use std::collections::HashMap;
use std::io::Error;
use bevy::app::App;
use bevy::log::{error};
use bevy::prelude::{First, IntoScheduleConfigs, Message, MessageWriter, Plugin};
use uuid::Uuid;
use shared::{NetResMut};
use shared::plugins::messaging::MessagingPlugin;
use shared::plugins::network::{CurrentNetworkSides, NetworkConnection, NetworkType, ServerConnection};

pub struct ServerNetworkPlugin;

#[derive(Message)]
pub struct ServerPortConnected{
    pub port_id: u32,
    pub connection_id: u32,
}

#[derive(Message)]
pub struct ServerPortDisconnected{
    pub port_id: u32,
    pub connection_id: u32,
    pub error: Option<Error>,
    pub was_connected: bool
}

#[derive(Message)]
pub struct AnonymousPeersAcceptedOnPort{
    pub port_id: u32,
    pub connection_id: u32,
    pub seasons_uuids: Vec<Uuid>
}

#[derive(Message)]
pub struct PeersDroppedServer{
    pub port_id: u32,
    pub connection_id: u32,
    pub peers: HashMap<Uuid,(Option<Uuid>,Error)>
}

impl Plugin for ServerNetworkPlugin {
    fn build(&self, app: &mut App) {
        let messaging_plugin_added = app.is_plugin_added::<MessagingPlugin>();

        let current_network_sides = app.world_mut().get_resource_mut::<CurrentNetworkSides>();

        if let Some(mut current_network_sides) = current_network_sides {
            if current_network_sides.side().contains(&NetworkType::Client){
                if messaging_plugin_added {
                    error!("Insert MessagingPlugin for last");
                    return;
                }

                current_network_sides.insert_side(NetworkType::LocalServer);
            }else{
                current_network_sides.insert_side(NetworkType::DedicatedServer);
            }
        }else{
            app.insert_resource(CurrentNetworkSides::new(Vec::from([
                NetworkType::DedicatedServer
            ])));
        }

        app.add_message::<ServerPortConnected>();
        app.add_message::<ServerPortDisconnected>();
        app.add_message::<AnonymousPeersAcceptedOnPort>();
        app.add_message::<PeersDroppedServer>();

        app.add_systems(First,(start_ports,check_peers_connected,listen_peers,check_peers_disconnected,check_port_disconnected).chain());
    }
}

pub fn start_ports(
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
    mut server_port_connected: MessageWriter<ServerPortConnected>,
){
    for (connection_id,server_connection) in &mut network_connection.0 {
        if let (Some(main_port), network_port_shared_infos) = server_connection.get_port_split(0) {
            let (started,started_now) = main_port.started();

            if started {
                if started_now {
                    println!("Connected main port");
                    server_port_connected.write(ServerPortConnected{
                        port_id: 0,
                        connection_id: *connection_id,
                    });
                }
            }else if let Some(network_port_shared_infos) = network_port_shared_infos {
                main_port.start(network_port_shared_infos);
            }
        }

        let ports_amount = server_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let (Some(port),network_port_shared_infos) = server_connection.get_port_split(port_id) {
                let (started,started_now) = port.started();

                if started {
                    if started_now {
                        println!("Connected secondary port");
                        server_port_connected.write(ServerPortConnected{
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

pub fn check_peers_connected(
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
    mut anonymous_peers_accepted_on_port: MessageWriter<AnonymousPeersAcceptedOnPort>,
){
    for (connection_id,server_connection) in &mut network_connection.0 {
        if let Some(main_port) = server_connection.get_port(0) {
            let peers_connected = main_port.peers_connected();
            
            if !peers_connected.is_empty() {
                anonymous_peers_accepted_on_port.write(AnonymousPeersAcceptedOnPort{
                    port_id: 0,
                    connection_id: *connection_id,
                    seasons_uuids: peers_connected,
                });
            }
        }

        let ports_amount = server_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let Some(port) = server_connection.get_port(port_id) {
                let peers_connected = port.peers_connected();

                if !peers_connected.is_empty() {
                    anonymous_peers_accepted_on_port.write(AnonymousPeersAcceptedOnPort{
                        port_id,
                        connection_id: *connection_id,
                        seasons_uuids: peers_connected,
                    });
                }
            }
        }
    }
}

pub fn listen_peers(
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
){
    for server_connection in &mut network_connection.0.values_mut() {
        if let (Some(main_port), network_port_shared_infos) = server_connection.get_port_split(0)
        && let Some(network_port_shared_infos) = network_port_shared_infos
        {
            main_port.listen_peers(network_port_shared_infos);
        }

        let ports_amount = server_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let (Some(port),network_port_shared_infos) = server_connection.get_port_split(port_id)
            && let Some(network_port_shared_infos) = network_port_shared_infos
            {
                port.listen_peers(network_port_shared_infos);
            }
        }
    }
}

pub fn check_peers_disconnected(
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
    mut peers_dropped_server: MessageWriter<PeersDroppedServer>
){
    for (connection_id,server_connection) in &mut network_connection.0 {
        if let Some(main_port) = server_connection.get_port(0) {
            let peers_dropped = main_port.get_peers_disconnected();

            if !peers_dropped.is_empty() {
              peers_dropped_server.write(PeersDroppedServer{
                  port_id: 0,
                  connection_id: *connection_id,
                  peers: peers_dropped,
              });
            }
        }

        let ports_amount = server_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let Some(port) = server_connection.get_port(port_id) {
                let peers_dropped = port.get_peers_disconnected();

                if !peers_dropped.is_empty() {
                    peers_dropped_server.write(PeersDroppedServer{
                        port_id,
                        connection_id: *connection_id,
                        peers: peers_dropped,
                    });
                }
            }
        }
    }
}

pub fn check_port_disconnected(
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
    mut server_port_disconnected: MessageWriter<ServerPortDisconnected>
){
    for (connection_id,server_connection) in &mut network_connection.0 {
        if let Some(main_port) = server_connection.get_port(0) {
            let (disconnected,error,was_connected) = main_port.disconnected();

            if disconnected {
                server_port_disconnected.write(ServerPortDisconnected{
                    port_id: 0,
                    connection_id: *connection_id,
                    error,
                    was_connected
                });
            }
        }

        let ports_amount = server_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let Some(port) = server_connection.get_port(port_id) {
                let (disconnected,error,was_connected) = port.disconnected();

                if disconnected {
                    server_port_disconnected.write(ServerPortDisconnected{
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