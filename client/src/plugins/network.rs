use std::io::Error;
use bevy::app::{App, First};
use bevy::log::error;
use bevy::prelude::{IntoScheduleConfigs, Message, MessageWriter, Plugin, Time};
use shared::{NetRes, NetResMut};
use shared::plugins::messaging::MessagingPlugin;
use shared::plugins::network::{ClientConnection, CurrentNetworkSides, NetworkConnection, NetworkType, ServerConnection};

pub struct ClientNetworkPlugin;

#[derive(Message)]
pub struct ClientPortConnected{
    pub port_id: u32,
    pub connection_id: u32,
}

#[derive(Message)]
pub struct ClientPortDisconnected{
    pub port_id: u32,
    pub connection_id: u32,
    pub error: Option<Error>
}

impl Plugin for ClientNetworkPlugin{
    fn build(&self, app: &mut App) {
        let messaging_plugin_added = if app.is_plugin_added::<MessagingPlugin>() {
            true
        } else { false };
        
        let current_network_sides = app.world_mut().get_resource_mut::<CurrentNetworkSides>();

        if let Some(mut current_network_sides) = current_network_sides {
            if messaging_plugin_added {
                error!("Insert MessagingPlugin for last");
                return;
            }
            
            current_network_sides.side().iter_mut().for_each(|network_type| {
                if network_type == &NetworkType::DedicatedServer{
                    *network_type = NetworkType::LocalServer;
                }
            });

            current_network_sides.insert_side(NetworkType::Client)
        }else{
            app.insert_resource(CurrentNetworkSides::new(Vec::from([
                NetworkType::Client
            ])));
        }

        app.add_message::<ClientPortConnected>();
        app.add_message::<ClientPortDisconnected>();
        app.add_systems(First,(start_ports,check_connected_ports,check_ports_down,start_listening_ports).chain());
    }
}

pub fn start_ports(
    mut network_connection: NetResMut<NetworkConnection<ClientConnection>>,
    server_network_connection: Option<NetRes<NetworkConnection<ServerConnection>>>,
    time: NetRes<Time>
){
    for (connection_id,client_connection) in &mut network_connection.0 {
        if let Some(server_network_connection) = server_network_connection.as_ref() {
            if let Some(_) = server_network_connection.0.get(connection_id) {
                continue;
            }
        }

        if let (Some(main_port),client_connection_shared_values) = client_connection.get_port_split(&0) {
            main_port.start(client_connection_shared_values, &time);
        }

        let ports_amount = client_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let (Some(port),client_connection_shared_values) = client_connection.get_port_split(&port_id) {
                port.start(client_connection_shared_values, &time);
            }
        }
    }
}

pub fn check_connected_ports(
    mut network_connection: NetResMut<NetworkConnection<ClientConnection>>,
    mut client_port_connected: MessageWriter<ClientPortConnected>
){
    for (connection_id,client_connection) in &mut network_connection.0 {
        if let Some(main_port) = client_connection.get_port(0) {
            if main_port.connected() {
                client_port_connected.write(ClientPortConnected{
                    port_id: 0,
                    connection_id: *connection_id,
                });
            }
        }

        for (port_id,port) in client_connection.secondary_ports.iter_mut(){
            if port.connected() {
                client_port_connected.write(ClientPortConnected{
                    port_id: *port_id,
                    connection_id: *connection_id,
                });
            }
        }
    }
}

pub fn check_ports_down(
    mut network_connection: NetResMut<NetworkConnection<ClientConnection>>,
    mut client_port_disconnected: MessageWriter<ClientPortDisconnected>,
    time: NetRes<Time>
){
    for (connection_id,client_connection) in &mut network_connection.0 {
        if let (Some(main_port),client_connection_shared_values) = client_connection.get_port_split(&0) {
            let (is_down, error) = main_port.check_connection_down();

            if is_down {
                client_port_disconnected.write(ClientPortDisconnected{
                    port_id: 0,
                    connection_id: *connection_id,
                    error
                });

                main_port.reconnect(client_connection_shared_values, &time);
            }
        }

        let ports_amount = client_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let (Some(port),client_connection_shared_values) = client_connection.get_port_split(&port_id) {
                let (is_down, error) = port.check_connection_down();

                if is_down {
                    client_port_disconnected.write(ClientPortDisconnected{
                        port_id,
                        connection_id: *connection_id,
                        error
                    });

                    port.reconnect(client_connection_shared_values, &time);
                }
            }
        }
    }
}

pub fn start_listening_ports(
    mut network_connection: NetResMut<NetworkConnection<ClientConnection>>,
){
    for (_,client_connection) in &mut network_connection.0 {
        if let (Some(main_port),client_connection_shared_values) = client_connection.get_port_split(&0) {
            main_port.start_listening(client_connection_shared_values);
        }

        let ports_amount = client_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let (Some(port),client_connection_shared_values) = client_connection.get_port_split(&port_id) {
                port.start_listening(client_connection_shared_values);
            }
        }
    }
}