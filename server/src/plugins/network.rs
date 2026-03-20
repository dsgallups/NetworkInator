use std::collections::HashMap;
use std::io::Error;
use std::sync::Arc;
use bevy::app::App;
use bevy::log::{error};
use bevy::prelude::{First, IntoScheduleConfigs, Message, MessageWriter, Plugin, Time};
use uuid::Uuid;
use shared::{NetRes, NetResMut};
use shared::plugins::authentication::{Authenticated, LocalPeerId, LocalSeasonUUID};
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
    pub error: Option<Error>
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
        let messaging_plugin_added = if app.is_plugin_added::<MessagingPlugin>() {
            true
        } else { false };

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

        app.add_systems(First,(set_local_player_as_connected,start_ports,check_connected_ports,check_ports_down,start_accepting_ports_connections,check_peers_disconnected,check_ports_peers_accepted,check_peers_queue_accepted,start_listening_peers).chain());
    }
}

pub fn set_local_player_as_connected(
    mut current_network_sides: NetResMut<CurrentNetworkSides>,
    local_season_uuid: Option<NetResMut<LocalSeasonUUID>>,
    local_peer_id: Option<NetResMut<LocalPeerId>>,
    authenticated: Option<MessageWriter<Authenticated>>,
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
){
    if !current_network_sides.side().contains(&NetworkType::LocalServer){
        return;
    }

    if let Some(mut local_season_uuid) = local_season_uuid {
        if local_season_uuid.0.is_none() {
            local_season_uuid.0 = Some(Uuid::new_v4());
        }

        if let Some(mut local_peer_id) = local_peer_id{
            if local_peer_id.0.is_none() {
                local_peer_id.0 = Some(Uuid::new_v4());
                
                if let Some(mut authenticated) = authenticated{
                    authenticated.write(Authenticated);
                }
            }

            for (_,server_connection) in network_connection.0.iter_mut(){
                if server_connection.peers_connected.contains_key(&local_season_uuid.0.unwrap()){
                    continue;
                }

                server_connection.peers_connected.insert(local_season_uuid.0.unwrap(),(Some(local_peer_id.0.unwrap()),true));
            }

        }
    }
}

pub fn start_ports(
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
    time: NetRes<Time>
){
    for (_,server_connection) in &mut network_connection.0 {
        if let (Some(main_port), server_connection_shared_values) = server_connection.get_port_split(&0) {
            main_port.start(server_connection_shared_values, &time);
        }

        let ports_amount = server_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let (Some(port),server_connection_shared_values) = server_connection.get_port_split(&port_id) {
                port.start(server_connection_shared_values, &time);
            }
        }
    }
}

pub fn check_connected_ports(
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
    mut server_port_connected: MessageWriter<ServerPortConnected>
){
    for (connection_id,server_connection) in &mut network_connection.0 {
        if let Some(main_port) = server_connection.get_port(0) {
            if main_port.connected() {
                server_port_connected.write(ServerPortConnected{
                    port_id: 0,
                    connection_id: *connection_id,
                });
            }
        }

        let ports_amount = server_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let (Some(port),_) = server_connection.get_port_split(&port_id) {
                if port.connected() {
                    server_port_connected.write(ServerPortConnected{
                        port_id,
                        connection_id: *connection_id,
                    });
                }
            }
        }
    }
}

pub fn check_ports_down(
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
    mut server_port_disconnected: MessageWriter<ServerPortDisconnected>,
    time: NetRes<Time>
){
    for (connection_id,server_connection) in &mut network_connection.0 {
        if let Some(main_port) = server_connection.get_port(0) {
            let (is_down, error) = main_port.check_connection_down();

            if is_down {
                server_port_disconnected.write(ServerPortDisconnected{
                    port_id: 0,
                    connection_id: *connection_id,
                    error
                });

                if let (Some(main_port),server_connection_shared_values) = server_connection.get_port_split(&0) {
                    main_port.reconnect(server_connection_shared_values, &time);
                }
            }
        }

        let ports_amount = server_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let (Some(port),server_connection_shared_values) = server_connection.get_port_split(&port_id) {
                let (is_down, error) = port.check_connection_down();

                if is_down {
                    server_port_disconnected.write(ServerPortDisconnected{
                        port_id,
                        connection_id: *connection_id,
                        error
                    });

                    port.reconnect(server_connection_shared_values, &time);
                }
            }
        }
    }
}

pub fn start_accepting_ports_connections(
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
){
    for (_,server_connection) in &mut network_connection.0 {
        if let (Some(main_port),server_connection_shared_values) = server_connection.get_port_split(&0) {
            main_port.accepting_connections(server_connection_shared_values);
        }

        let ports_amount = server_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let (Some(port),server_connection_shared_values) = server_connection.get_port_split(&port_id) {
                port.accepting_connections(server_connection_shared_values);
            }
        }
    }
}

pub fn check_peers_disconnected(
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
    mut peer_dropped_server: MessageWriter<PeersDroppedServer>,
){
    for (connection_id,server_connection) in &mut network_connection.0 {
        if let Some(main_port) = server_connection.get_port(0) {
            let peers_disconnected = main_port.check_peers_dropped();
            
            if server_connection.is_authentication_connection() {
                for (uuid,(_,_)) in peers_disconnected.iter() {
                    server_connection.peers_connected.remove(uuid);
                }
            }
            
            peer_dropped_server.write(PeersDroppedServer{
                port_id: 0,
                connection_id: *connection_id,
                peers: peers_disconnected,
            });
        }

        let ports_amount = server_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let (Some(port),_) = server_connection.get_port_split(&port_id) {
                let peers_disconnected = port.check_peers_dropped();

                peer_dropped_server.write(PeersDroppedServer{
                    port_id,
                    connection_id: *connection_id,
                    peers: peers_disconnected,
                });
            }
        }
    }
}

pub fn check_ports_peers_accepted(
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
    mut peers_accepted_on_port: MessageWriter<AnonymousPeersAcceptedOnPort>
){
    for (connection_id,server_connection) in &mut network_connection.0 {
        let semaphore = server_connection.semaphore();
        let disconnect_peer_if_full = server_connection.disconnect_peer_if_full();

        if let Some(main_port) = server_connection.get_port(0) {
            let seasons_uuids = main_port.anonymous_peers_accepted(semaphore,disconnect_peer_if_full);

            if server_connection.is_authentication_connection() {
                for uuid in &seasons_uuids {
                    server_connection.peers_connected.insert(*uuid,(None,false));
                }
            }

            peers_accepted_on_port.write(AnonymousPeersAcceptedOnPort{
                port_id: 0,
                connection_id: *connection_id,
                seasons_uuids
            });
        }

        let ports_amount = server_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            let semaphore = server_connection.semaphore();

            if let (Some(port),_) = server_connection.get_port_split(&port_id) {
                let seasons_uuids = port.anonymous_peers_accepted(semaphore,disconnect_peer_if_full);

                peers_accepted_on_port.write(AnonymousPeersAcceptedOnPort{
                    port_id,
                    connection_id: *connection_id,
                    seasons_uuids
                });
            }
        }
    }
}

pub fn check_peers_queue_accepted(
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
    mut peers_accepted_on_port: MessageWriter<AnonymousPeersAcceptedOnPort>
){
    for (connection_id,server_connection) in &mut network_connection.0 {
        if let Some(semaphore) = server_connection.semaphore(){
            if let Some(main_port) = server_connection.get_port(0) {
                let seasons_uuids = main_port.check_peers_on_queue_accepted(Arc::clone(&semaphore));

                peers_accepted_on_port.write(AnonymousPeersAcceptedOnPort{
                    port_id: 0,
                    connection_id: *connection_id,
                    seasons_uuids
                });
            }

            let ports_amount = server_connection.get_ports_amount();

            for port_id in 1..=ports_amount {
                if let (Some(port),_) = server_connection.get_port_split(&port_id) {
                    let seasons_uuids = port.check_peers_on_queue_accepted(Arc::clone(&semaphore));

                    peers_accepted_on_port.write(AnonymousPeersAcceptedOnPort{
                        port_id,
                        connection_id: *connection_id,
                        seasons_uuids
                    });
                }
            }
        }else{
            continue;
        }
    }
}

pub fn start_listening_peers(
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
){
    for (_,server_connection) in &mut network_connection.0 {
        if let (Some(main_port),server_connection_shared_values) = server_connection.get_port_split(&0) {
            main_port.start_listening_anonymous_peers(server_connection_shared_values);
            main_port.start_listening_authenticated_peers(server_connection_shared_values);
        }

        let ports_amount = server_connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let (Some(port),server_connection_shared_values) = server_connection.get_port_split(&port_id) {
                port.start_listening_anonymous_peers(server_connection_shared_values);
                port.start_listening_authenticated_peers(server_connection_shared_values);
            }
        }
    }
}