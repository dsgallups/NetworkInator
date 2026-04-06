use crate::plugins::messaging::{ClientConnectionParams, MessageReceivedFromAnonymousPeer, MessageReceivedFromPeer, MessageReceivedFromServer, MessageTrait, MessageTraitPlugin, ServerConnectionParams};
use std::collections::HashMap;
use bevy::app::App;
use bevy::asset::uuid::Uuid;
use bevy::prelude::{IntoScheduleConfigs, Message, MessageReader, MessageWriter, Plugin, PreUpdate, Resource, Update};
use serde::{Deserialize, Serialize};
use message_pro_macro::ConnectionMessage;
use crate::{NetRes, NetResMut};
use crate::plugins::network::{ClientConnection, CurrentNetworkSides, NetworkConnection, NetworkType, ServerConnection};

pub struct AuthenticationPlugin;

#[derive(Resource,Default)]
pub struct AuthenticatedSessions(HashMap<Uuid,Uuid>);

#[derive(Serialize,Deserialize,ConnectionMessage)]
#[connection_message(authentication = true)]
struct AuthenticationMessage{
    session_uuid: Option<Uuid>,
}

#[derive(Serialize,Deserialize,ConnectionMessage)]
struct AuthenticatedFromServer{
    session_uuid: Uuid,
    peer_uuid: Uuid
}

#[derive(Resource,Default)]
pub struct LocalPeerUUID(Option<Uuid>);

#[derive(Resource,Default)]
pub struct LocalSeasonUUID(Option<Uuid>);

#[derive(Message)]
pub struct ClientPortAuthenticated{
    pub port_id: u32,
    pub connection_id: u32
}

impl Plugin for AuthenticationPlugin{
    fn build(&self, app: &mut App) {
        let (is_client, is_local_server, is_dedicated_server) = {
            let world = app.world();
            let sides = world.get_resource::<CurrentNetworkSides>()
                .expect("Insert ServerNetworkPlugin or ClientNetworkPlugin first, if its a LocalServer insert both first");
            (
                sides.0.contains(&NetworkType::Client),
                sides.0.contains(&NetworkType::LocalServer),
                sides.0.contains(&NetworkType::DedicatedServer)
            )
        };

        app.register_message::<AuthenticationMessage>();
        app.register_message::<AuthenticatedFromServer>();

        if is_dedicated_server || is_local_server {
            app.init_resource::<AuthenticatedSessions>();
            app.add_systems(PreUpdate,(check_peer_authenticated,check_authenticated_peers_are_connected).chain());
            
            if is_local_server {
                app.add_systems(Update,authenticate_local_peer);
            }
        }

        if is_client {
            app.add_message::<ClientPortAuthenticated>();
            app.init_resource::<LocalPeerUUID>();
            app.init_resource::<LocalSeasonUUID>();
            app.add_systems(Update,(authenticate_port,check_port_authenticated).chain());
        }
    }
}

fn authenticate_local_peer(
    mut local_peer_uuid: NetResMut<LocalPeerUUID>,
    mut local_season_uuid: NetResMut<LocalSeasonUUID>,
    mut authenticated_sessions: NetResMut<AuthenticatedSessions>,
    mut server_network_connection: NetResMut<NetworkConnection<ServerConnection>>,
    mut client_network_connection: NetResMut<NetworkConnection<ClientConnection>>,
){
    if local_peer_uuid.0.is_none() {
        let new_peer_uuid = Uuid::new_v4();
        let new_season_uuid = Uuid::new_v4();
        
        local_peer_uuid.0 = Some(new_peer_uuid);
        local_season_uuid.0 = Some(new_season_uuid);

        authenticated_sessions.0.insert(new_season_uuid,new_peer_uuid);
        
        println!("Local peer authenticated");
    }
    
    let session_uuid = local_season_uuid.0.unwrap();
    let current_peer_uuid = local_peer_uuid.0.unwrap();
    
    for (_,connection) in server_network_connection.0.iter_mut(){
        if let Some(main_port) = connection.get_port(0) && !main_port.is_season_authenticated(&session_uuid) {
            main_port.authenticate_peer(session_uuid, current_peer_uuid, Some(session_uuid));
        }

        let ports_amount = connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let Some(port) = connection.get_port(port_id) && !port.is_season_authenticated(&session_uuid){
                port.authenticate_peer(session_uuid, current_peer_uuid, Some(session_uuid));
            }
        }
    }

    for (_,connection) in client_network_connection.0.iter_mut(){
        if let Some(main_port) = connection.get_port(0) && !main_port.is_port_authenticated(){
            main_port.authenticate_port();
        }

        let ports_amount = connection.get_ports_amount();

        for port_id in 1..=ports_amount {
            if let Some(port) = connection.get_port(port_id) && !port.is_port_authenticated(){
                port.authenticate_port();
            }
        }
    }
}

fn check_peer_authenticated(
    mut message_received_from_anonymous_peer: MessageReader<MessageReceivedFromAnonymousPeer<AuthenticationMessage>>,
    mut message_received_from_authenticated_peer: MessageReader<MessageReceivedFromPeer<AuthenticationMessage>>,
    mut authenticated_sessions: NetResMut<AuthenticatedSessions>,
    mut server_connections_params: ServerConnectionParams
){
    for message in message_received_from_anonymous_peer.read() {
        let auth_message = &message.message;

        if let Some(connection) = server_connections_params.get_connections().0.get_mut(&message.connection_id) {
            let is_authentication_connection = connection.is_authentication_connection();

            if let Some(port) = connection.get_port(message.port_id) {
                if !port.is_port_authenticate_able() { continue; }

                if let Some(session_uuid) = auth_message.session_uuid {
                    if let Some(current_peer_uuid) = authenticated_sessions.0.get(&session_uuid) && !port.is_season_authenticated(&session_uuid) {
                        port.authenticate_peer(message.session_uuid, *current_peer_uuid, Some(session_uuid));

                        server_connections_params.send_message::<AuthenticatedFromServer>(message.connection_id, message.port_id, &AuthenticatedFromServer{
                            session_uuid,
                            peer_uuid: *current_peer_uuid,
                        }, *current_peer_uuid, None);
                    }
                }else if is_authentication_connection && port.is_main_port() && !port.is_season_authenticated(&message.session_uuid) {
                    let peer_uuid = Uuid::new_v4();

                    port.authenticate_peer(message.session_uuid, peer_uuid, None);
                    authenticated_sessions.0.insert(message.session_uuid, peer_uuid);

                    server_connections_params.send_message::<AuthenticatedFromServer>(message.connection_id, message.port_id, &AuthenticatedFromServer{
                        session_uuid: message.session_uuid,
                        peer_uuid,
                    }, peer_uuid, None);
                }
            }
        }
    }

    for message in message_received_from_authenticated_peer.read() {
        let auth_message = &message.message;

        if let Some(connection) = server_connections_params.get_connections().0.get_mut(&message.connection_id)
        && let Some(_) = connection.get_port(message.port_id) && let Some(session_uuid) = auth_message.session_uuid
        && let Some(current_peer_uuid) = authenticated_sessions.0.get(&session_uuid)

        {
            server_connections_params.send_message::<AuthenticatedFromServer>(message.connection_id, message.port_id, &AuthenticatedFromServer{
                session_uuid,
                peer_uuid: *current_peer_uuid,
            }, *current_peer_uuid, None);
        }
    }
}

fn check_authenticated_peers_are_connected(
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
    mut authenticated_sessions: NetResMut<AuthenticatedSessions>,
    local_peer_uuid: Option<NetRes<LocalPeerUUID>>,
){
    authenticated_sessions.0.retain(|_, peer_uuid| {
        if let Some(local_peer_uuid) = &local_peer_uuid && let Some(local_peer_uuid) = &local_peer_uuid.0 && local_peer_uuid == peer_uuid {
            return true;
        }
        
        for (_,connection) in network_connection.0.iter_mut() {
            if !connection.is_authentication_connection() { continue }

            if let Some(main_port) = connection.get_port(0) {
                if main_port.is_peer_connected(peer_uuid) { continue }

                return false
            }
        }

        true
    });
}

fn check_port_authenticated(
    mut authenticated_from_server: MessageReader<MessageReceivedFromServer<AuthenticatedFromServer>>,
    mut network_connection: NetResMut<NetworkConnection<ClientConnection>>,
    mut local_peer_uuid: NetResMut<LocalPeerUUID>,
    mut local_season_uuid: NetResMut<LocalSeasonUUID>,
    mut client_port_authenticated: MessageWriter<ClientPortAuthenticated>,
){
    for ev in authenticated_from_server.read(){
        let authenticated_from_server = &ev.message;
        let port_id = ev.port_id;
        let connection_id = ev.connection_id;

        local_peer_uuid.0 = Some(authenticated_from_server.peer_uuid);
        local_season_uuid.0 = Some(authenticated_from_server.session_uuid);

        if let Some(connection) = network_connection.0.get_mut(&connection_id) && let Some(port) = connection.get_port(port_id)
        && !port.is_port_authenticated()
        {
            port.authenticate_port();

            client_port_authenticated.write(ClientPortAuthenticated{
                port_id,
                connection_id,
            });
        }
    }
}

fn authenticate_port(
    mut client_connections_params: ClientConnectionParams,
    local_season_uuid: NetRes<LocalSeasonUUID>,
){
    let network_connections = client_connections_params.get_connections();
    let mut pending: HashMap<u32,Vec<u32>> = HashMap::new();

    for (connection_id,connection) in network_connections.0.iter_mut(){
        let is_authentication_connection = connection.is_authentication_connection();

        if local_season_uuid.0.is_none() {
            if !is_authentication_connection { continue }

            pending.insert(*connection_id,Vec::from([0]));
        }else {
            if let Some(main_port) = connection.get_port(0) && !main_port.is_port_authenticated() {
                pending.insert(*connection_id,Vec::from([0]));
            }

            let ports_amount = connection.get_ports_amount();

            if !pending.contains_key(connection_id) {
                pending.insert(*connection_id,Vec::new());
            }

            let vec = pending.get_mut(connection_id).unwrap();

            for port_id in 1..=ports_amount {
                if let Some(port) = connection.get_port(port_id) && !port.is_port_authenticated() {
                    vec.push(port_id);
                }
            }
        }
    }

    for (connection_id,ports) in pending.into_iter() {
        for port_id in ports.iter() {
            client_connections_params.send_message::<AuthenticationMessage>(connection_id, *port_id, &AuthenticationMessage{
                session_uuid: local_season_uuid.0,
            }, None);
        }
    }
}