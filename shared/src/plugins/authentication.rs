use std::collections::HashMap;
use crate::plugins::messaging::{MessageReceivedFromClient, MessageReceivedFromServer, MessageTrait, MessageTraitPlugin};
use bevy::app::App;
use bevy::asset::uuid::Uuid;
use bevy::prelude::{IntoScheduleConfigs, Message, MessageReader, MessageWriter, Plugin, PreUpdate, Resource, Time};
use serde::{Deserialize, Serialize};
use message_pro_macro::ConnectionMessage;
use crate::{NetRes, NetResMut};
use crate::plugins::network::{ClientConnection, CurrentNetworkSides, NetworkConnection, NetworkType, ServerConnection};

pub struct AuthenticationPlugin;

pub struct SeasonInfos{
    peer_id: Uuid,
    season_duration: f32,
    disconnected_duration: Option<f32>,
    connected: bool,
}

#[derive(Resource,Default)]
struct AuthenticatedSessions(HashMap<Uuid,SeasonInfos>,HashMap<Uuid,Uuid>);

#[derive(Serialize,Deserialize,ConnectionMessage)]
#[connection_message(authentication = true)]
struct AuthenticationMessage{
    pub first_join: bool,
    pub session_uuid: Option<Uuid>,
}

#[derive(Serialize,Deserialize,ConnectionMessage)]
#[connection_message(authentication = true)]
struct AuthenticatePort{
    pub session_uuid: Uuid
}

#[derive(Serialize,Deserialize,ConnectionMessage)]
struct AuthenticatedMessage(Uuid,Uuid);

#[derive(Serialize,Deserialize,ConnectionMessage)]
struct PortAuthenticated;

#[derive(Resource,Default)]
pub struct LocalSeasonUUID(pub Option<Uuid>);

#[derive(Resource,Default)]
pub struct LocalPeerId(pub Option<Uuid>);

#[derive(Message)]
pub struct Authenticated;

#[derive(Message)]
pub struct PeerAuthenticated {
    pub session_uuid: Uuid,
    pub peer_uuid: Uuid,
    pub port_id: u32,
    pub connection_id: u32
}

impl Plugin for AuthenticationPlugin {
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
        app.register_message::<AuthenticatePort>();
        app.register_message::<AuthenticatedMessage>();
        app.register_message::<PortAuthenticated>();

        if is_dedicated_server || is_local_server {
            app.init_resource::<AuthenticatedSessions>();
            app.add_message::<PeerAuthenticated>();
            app.add_systems(PreUpdate,(check_seasons_ended,check_peer_authenticating,authenticate_ports).chain());
        }

        if is_client{
            app.init_resource::<LocalSeasonUUID>();
            app.init_resource::<LocalPeerId>();
            app.add_message::<Authenticated>();
            app.add_systems(PreUpdate,(check_local_peer_and_season_received,authenticate,check_client_ports_authenticated).chain());
        }
    }
}

fn check_seasons_ended(
    network_connection: NetRes<NetworkConnection<ServerConnection>>,
    mut authenticated_sessions: NetResMut<AuthenticatedSessions>,
    time: NetRes<Time>
){
    for (_,server_connection) in network_connection.0.iter(){
        for (season_uuid,season_infos) in authenticated_sessions.0.iter_mut() {
            let (connected,authenticated) = server_connection.is_peer_connected(season_uuid);

            if !connected || !authenticated {
                season_infos.connected = false;
                season_infos.disconnected_duration = Some(time.elapsed_secs())
            }else if season_infos.disconnected_duration.is_some() {
                season_infos.disconnected_duration = None;
            }
        }
    }
    
    authenticated_sessions.0.retain(|_,season_infos|{
        if season_infos.connected { return true }

        if let Some(disconnected_duration) = season_infos.disconnected_duration {
            let elapsed = time.elapsed_secs() - disconnected_duration;

            if elapsed >= 60.0 {
                false
            }else{
                true
            }
        }else { 
            true
        }
    });
}

fn authenticate_ports(
    mut message_received_from_client: MessageReader<MessageReceivedFromClient<AuthenticatePort>>,
    mut authenticated_sessions: NetResMut<AuthenticatedSessions>,
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
    mut peer_authenticated: MessageWriter<PeerAuthenticated>,
){
    for ev in message_received_from_client.read() {
        let authenticated_port = &ev.message;
        let session_uuid = &authenticated_port.session_uuid;
        let port_id = ev.port_id;
        let connection_id = ev.connection_id;

        if let Some(season_infos) = authenticated_sessions.0.get_mut(session_uuid) {
            if let Some(server_connection) = network_connection.0.get_mut(&connection_id){
                if let Some(port) = server_connection.get_port(port_id){
                    if port.authenticate_peer(ev.session_id,season_infos.peer_id,Some(*session_uuid)) {
                        peer_authenticated.write(PeerAuthenticated{
                            session_uuid: *session_uuid,
                            peer_uuid: season_infos.peer_id,
                            port_id: ev.port_id,
                            connection_id: ev.connection_id,
                        });
                    }
                }

                server_connection.send_peer_message::<PortAuthenticated>(Box::new(PortAuthenticated), port_id, session_uuid, None);
            }
        }
    }
}

fn check_peer_authenticating(
    mut message_received_from_client: MessageReader<MessageReceivedFromClient<AuthenticationMessage>>,
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
    mut peer_authenticated: MessageWriter<PeerAuthenticated>,
    mut authenticated_sessions: NetResMut<AuthenticatedSessions>,
    time: NetRes<Time>
){

    let mut all_authenticated : HashMap<Uuid, (Uuid,u32)> = HashMap::new();

    for ev in message_received_from_client.read() {
        let authenticated_message = &ev.message;

        if ev.port_id != 0 { continue }

        match network_connection.0.get_mut(&ev.connection_id) {
            Some(server_connection) => {
                let is_authentication_connection = server_connection.is_authentication_connection();

                match server_connection.get_port(ev.port_id) {
                    Some(port) => {
                        if authenticated_message.first_join && is_authentication_connection {
                            let peer_uuid = Uuid::new_v4();
                            let mut session_to_use = ev.session_id;

                            if let Some(current_season_uuid) = authenticated_sessions.1.get(&peer_uuid) {
                                session_to_use = *current_season_uuid;
                            }

                            if let Some(season_infos) = authenticated_sessions.0.get(&session_to_use) {
                                if !season_infos.connected {
                                    if is_authentication_connection {
                                        authenticated_sessions.0.insert(session_to_use,SeasonInfos{
                                            peer_id: peer_uuid,
                                            season_duration: time.elapsed_secs(),
                                            disconnected_duration: None,
                                            connected: true,
                                        });

                                        authenticated_sessions.1.insert(peer_uuid,session_to_use);
                                    }

                                    if port.authenticate_peer(session_to_use,peer_uuid,None) {
                                        all_authenticated.insert(session_to_use, (peer_uuid,ev.connection_id));

                                        peer_authenticated.write(PeerAuthenticated{
                                            session_uuid: session_to_use,
                                            peer_uuid,
                                            port_id: ev.port_id,
                                            connection_id: ev.connection_id,
                                        });
                                    };
                                }
                            }else{
                                if is_authentication_connection {
                                    authenticated_sessions.0.insert(session_to_use,SeasonInfos{
                                        peer_id: peer_uuid,
                                        season_duration: time.elapsed_secs(),
                                        disconnected_duration: None,
                                        connected: true
                                    });

                                    authenticated_sessions.1.insert(peer_uuid,session_to_use);
                                }

                                if port.authenticate_peer(session_to_use,peer_uuid,None) {
                                    all_authenticated.insert(session_to_use, (peer_uuid,ev.connection_id));

                                    peer_authenticated.write(PeerAuthenticated{
                                        session_uuid: session_to_use,
                                        peer_uuid,
                                        port_id: ev.port_id,
                                        connection_id: ev.connection_id,
                                    });
                                };
                            }

                            if ev.port_id == 0 {
                                let authenticated_message = Box::new(AuthenticatedMessage(session_to_use,peer_uuid));
                                server_connection.send_peer_message::<AuthenticatedMessage>(authenticated_message, 0, &session_to_use, None);
                            }
                        }else{
                            if let Some(authenticated_session_id) = authenticated_message.session_uuid {
                                if let Some(season_infos) = authenticated_sessions.0.get_mut(&authenticated_session_id) {
                                    if season_infos.connected { continue; }

                                    if port.authenticate_peer(ev.session_id, season_infos.peer_id, Some(authenticated_session_id)) {
                                        peer_authenticated.write(PeerAuthenticated{
                                            session_uuid: authenticated_session_id,
                                            peer_uuid: season_infos.peer_id,
                                            port_id: ev.port_id,
                                            connection_id: ev.connection_id,
                                        });
                                    };

                                    season_infos.connected = true;
                                }
                            }
                        }
                    }
                    None => {}
                }
            }
            None => {
                continue;
            }
        }
    }

    for (season_uuid,(peer_uuid,connection_id)) in all_authenticated.iter() {
        if let Some(server_connection) = network_connection.0.get_mut(connection_id) {
            server_connection.peers_connected.insert(*season_uuid, (Some(*peer_uuid), false));
        }
    }
}

fn authenticate(
    mut network_connection: NetResMut<NetworkConnection<ClientConnection>>,
    local_season_uuid: NetRes<LocalSeasonUUID>,
){
    for (_,client_connection) in network_connection.0.iter_mut() {
        let mut is_main_port_authenticated = false;

        if local_season_uuid.0.is_none() {
            if !client_connection.is_authentication_connection() {
                continue;
            }

            client_connection.send_server_message::<AuthenticationMessage>(Box::new(AuthenticationMessage{
                first_join: true,
                session_uuid: None,
            }),0, None)
        }else {
            if let Some(main_port) = client_connection.get_port(0) {
                if main_port.is_port_authenticated() {
                    is_main_port_authenticated = true;
                }else {
                    let authentication_message = AuthenticationMessage{
                        first_join: true,
                        session_uuid: local_season_uuid.0,
                    };

                    client_connection.send_server_message::<AuthenticationMessage>(Box::new(authentication_message),0, None);
                }
            }

            if is_main_port_authenticated {
                let ports_amount = client_connection.get_ports_amount();

                for port_id in 1..=ports_amount {
                    if let (Some(port),_) = client_connection.get_port_split(&port_id) {
                        if port.is_port_authenticated() {
                            continue;
                        }

                        let authenticate_port = AuthenticatePort{
                            session_uuid: local_season_uuid.0.unwrap(),
                        };

                        client_connection.send_server_message::<AuthenticatePort>(Box::new(authenticate_port),port_id, None);
                    }
                }
            }
        }
    }
}

fn check_local_peer_and_season_received(
    mut authenticated_message: MessageReader<MessageReceivedFromServer<AuthenticatedMessage>>,
    mut local_season_uuid: NetResMut<LocalSeasonUUID>,
    mut local_peer_id: NetResMut<LocalPeerId>,
    mut authenticated: MessageWriter<Authenticated>,
    mut network_connection: NetResMut<NetworkConnection<ClientConnection>>,
){
    for ev in authenticated_message.read() {
        let authenticated_message = &ev.message;

        local_season_uuid.0 = Some(authenticated_message.0);
        local_peer_id.0 = Some(authenticated_message.1);

        if let Some(client_connection) = network_connection.0.get_mut(&ev.connection_id){
            if let Some(port) = client_connection.get_port(ev.port_id) {
                port.set_port_authenticated();
            }
        }

        authenticated.write(Authenticated);
    }
}

fn check_client_ports_authenticated(
    mut port_authenticated_message: MessageReader<MessageReceivedFromServer<PortAuthenticated>>,
    mut network_connection: NetResMut<NetworkConnection<ClientConnection>>,
){
    for ev in port_authenticated_message.read() {
        if let Some(client_connection) = network_connection.0.get_mut(&ev.connection_id){
            if let Some(port) = client_connection.get_port(ev.port_id) {
                port.set_port_authenticated();
            }
        }
    }
}