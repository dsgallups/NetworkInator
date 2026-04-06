use std::io::Error;
use bevy::app::App;
use bevy::prelude::{error, First, IntoScheduleConfigs, Message, MessageWriter, Plugin};
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
    pub error: Option<Error>,
    pub was_connected: bool
}

impl Plugin for ClientNetworkPlugin {
    fn build(&self, app: &mut App) {
        let messaging_plugin_added = app.is_plugin_added::<MessagingPlugin>();

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
        app.add_systems(First,(start_ports,start_listening_ports,check_port_disconnected).chain());
    }
}

pub fn start_ports(
    mut network_connection: NetResMut<NetworkConnection<ClientConnection>>,
    mut client_port_connected: MessageWriter<ClientPortConnected>,
    server_network_connection: Option<NetRes<NetworkConnection<ServerConnection>>>,
){
    for (connection_id,client_connection) in &mut network_connection.0 {
        if let Some(server_network_connection) = server_network_connection.as_ref() && server_network_connection.0.contains_key(connection_id) {
            continue;
        }

        if let (Some(main_port), network_port_shared_infos) = client_connection.get_port_split(0) {
            let (started,started_now) = main_port.started();

            if started {
                if started_now {
                    client_port_connected.write(ClientPortConnected{
                        port_id: 0,
                        connection_id: *connection_id,
                    });
                }
            }else if let Some(network_port_shared_infos) = network_port_shared_infos{
                main_port.start(network_port_shared_infos);
            }
        }

        let ports_amount = client_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let (Some(port),network_port_shared_infos) = client_connection.get_port_split(port_id) {
                let (started,started_now) = port.started();

                if started {
                    if started_now {
                        client_port_connected.write(ClientPortConnected{
                            port_id,
                            connection_id: *connection_id,
                        });
                    }
                    continue
                }

                if let Some(network_port_shared_infos) = network_port_shared_infos{
                    port.start(network_port_shared_infos);
                }
            }
        }
    }
}

pub fn start_listening_ports(
    mut network_connection: NetResMut<NetworkConnection<ClientConnection>>,
){
    for (_,client_connection) in &mut network_connection.0.iter_mut() {
        if let (Some(main_port), network_port_shared_infos) = client_connection.get_port_split(0) && let Some(network_port_shared_infos) = network_port_shared_infos {
            main_port.listen_to_server(network_port_shared_infos);
        }

        let ports_amount = client_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let (Some(port), network_port_shared_infos) = client_connection.get_port_split(port_id) && let Some(network_port_shared_infos) = network_port_shared_infos {
                port.listen_to_server(network_port_shared_infos);
            }
        }
    }
}

pub fn check_port_disconnected(
    mut network_connection: NetResMut<NetworkConnection<ClientConnection>>,
    mut client_port_disconnected: MessageWriter<ClientPortDisconnected>
){
    for (connection_id,client_connection) in &mut network_connection.0 {
        if let Some(main_port) = client_connection.get_port(0) {
            let (disconnected,error,was_connected) = main_port.disconnected();

            if disconnected {
                client_port_disconnected.write(ClientPortDisconnected{
                    port_id: 0,
                    connection_id: *connection_id,
                    error,
                    was_connected
                });
            }
        }

        let ports_amount = client_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let Some(port) = client_connection.get_port(port_id) {
                let (disconnected,error,was_connected) = port.disconnected();

                if disconnected {
                    client_port_disconnected.write(ClientPortDisconnected{
                        port_id,
                        connection_id: *connection_id,
                        error,
                        was_connected
                    });
                }
            }
        }
    }
}