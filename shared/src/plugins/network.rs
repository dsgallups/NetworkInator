use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::io::Error;
use std::sync::Arc;
use bevy::app::App;
use bevy::asset::uuid::Uuid;
use bevy::prelude::{Plugin, Res, Resource, Time};
use tokio::runtime::Runtime;
use tokio::sync::{Semaphore};
use crate::plugins::messaging::{MessageTrait, MESSAGE_REGISTRY_TYPE_ID};
pub struct NetworkPlugin;

#[cfg(target_arch = "wasm32")]
pub trait ServerPort{

}

#[cfg(target_arch = "wasm32")]
pub trait ClientPort{
    
}

#[derive(Default)]
pub struct ServerConnectionSharedValues{
    runtime: Option<Runtime>,
}

#[derive(Default)]
pub struct ClientConnectionSharedValues{
    runtime: Option<Runtime>,
}

pub enum PortSendType{
    Reliable,
    Unreliable
}

pub trait PortInfosTrait{}

pub trait SendMessageArgs: Any{}

pub trait ClientPort: Send + Sync{
    fn start(&mut self, client_connection_shared_values: &ClientConnectionSharedValues, time: &Res<Time>);
    fn as_main_port(&mut self) -> bool;
    fn reconnect(&mut self, client_connection_shared_values: &ClientConnectionSharedValues, time: &Res<Time>);
    fn check_connection_down(&mut self) -> (bool, Option<Error>);
    fn connected(&mut self) -> bool;
    fn disconnect(&mut self);
    fn start_listening(&mut self, client_connection_shared_values: &ClientConnectionSharedValues);
    fn get_connecting_status(&self) -> (bool,bool,f32);
    fn get_port_infos(&self) -> &dyn PortInfosTrait;
    fn send_message_for_server(&mut self, client_connection_shared_values: &ClientConnectionSharedValues, message: &dyn MessageTrait, message_id: u32, send_message_args: Option<Box<dyn SendMessageArgs>>);
    fn get_server_messages(&mut self) -> Vec<Vec<u8>>;
    fn is_port_authenticated(&self) -> bool;
    fn set_port_authenticated(&mut self);
    fn is_main_port(&self) -> bool;
    fn get_port_send_type(&self) -> &PortSendType;
}
pub trait ServerPort: Send + Sync{
    fn start(&mut self, server_connection_shared_values: &ServerConnectionSharedValues, time: &Res<Time>);
    fn as_main_port(&mut self) -> bool;
    fn accepting_connections(&mut self, server_connection_shared_values: &ServerConnectionSharedValues);
    fn anonymous_peers_accepted(&mut self, semaphore: Option<Arc<Semaphore>>, disconnect_peer_if_full: bool) -> Vec<Uuid>;
    fn reconnect(&mut self, server_connection_shared_values: &ServerConnectionSharedValues, time: &Res<Time>);
    fn check_connection_down(&mut self) -> (bool, Option<Error>);
    fn connected(&mut self) -> bool;
    fn disconnect(&mut self);
    fn get_connecting_status(&self) -> (bool,bool,f32);
    fn check_peers_dropped(&mut self) -> HashMap<Uuid,(Option<Uuid>,Error)>;
    fn check_peers_on_queue_accepted(&mut self, semaphore: Arc<Semaphore>) -> Vec<Uuid>;
    fn start_listening_anonymous_peers(&mut self, server_connection_shared_values: &ServerConnectionSharedValues);
    fn start_listening_authenticated_peers(&mut self, server_connection_shared_values: &ServerConnectionSharedValues);
    fn get_peers_messages(&mut self) -> Vec<(Uuid,Option<Uuid>,Vec<u8>)>;
    fn authenticate_peer(&mut self, season_uuid: Uuid, peer_id: Uuid, new_season_uuid: Option<Uuid>) -> bool;
    fn send_message_for_peer(&mut self, server_connection_shared_values: &ServerConnectionSharedValues, season_uuid: &Uuid, message: &dyn MessageTrait, message_id: u32, send_message_args: Option<Box<dyn SendMessageArgs>>);
    fn is_peer_connected(&self, season_uuid: &Uuid) -> (bool, bool);
    fn get_port_infos(&self) -> &dyn PortInfosTrait;
    fn disconnect_peer(&mut self, season_uuid: &Uuid);
    fn is_main_port(&self) -> bool;
    fn get_port_send_type(&self) -> &PortSendType;
}

pub trait ClientSettingsPort{
    fn create_port(self: Box<Self>) -> Box<dyn ClientPort>;
}
pub trait ServerSettingsPort{
    fn create_port(self: Box<Self>) -> Box<dyn ServerPort>;
}

#[derive(Eq, PartialEq)]
pub enum NetworkType {
    Client,
    DedicatedServer,
    LocalServer
}

#[allow(dead_code)]
#[derive(Default)]
pub struct ServerConnection{
    main_port: Option<Box<dyn ServerPort>>,
    pub secondary_ports: HashMap<u32, Box<dyn ServerPort>>,
    server_connection_shared_values: ServerConnectionSharedValues,
    max_connections: i32,
    disconnect_peer_if_full: bool,
    semaphore: Option<Arc<Semaphore>>,
    pub peers_connected: HashMap<Uuid,(Option<Uuid>,bool)>,
    is_authentication_connection: bool,
}
#[derive(Default)]
pub struct ClientConnection{
    main_port: Option<Box<dyn ClientPort>>,
    pub secondary_ports: HashMap<u32, Box<dyn ClientPort>>,
    client_connection_shared_values: ClientConnectionSharedValues,
    is_authentication_connection: bool
}

#[derive(Resource)]
pub struct CurrentNetworkSides(pub(crate) Vec<NetworkType>);

#[derive(Resource,Default)]
pub struct NetworkConnection<T>(pub HashMap<u32, T>);

impl CurrentNetworkSides {
    pub fn new(sides: Vec<NetworkType>) -> CurrentNetworkSides {
        CurrentNetworkSides(sides)
    }

    pub fn side(&mut self) -> &mut Vec<NetworkType>{
        &mut self.0
    }

    pub fn insert_side(&mut self, side: NetworkType){
        self.0.push(side);
    }
}

impl Plugin for NetworkPlugin {
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

        if is_client || is_local_server {
            #[cfg(target_arch = "wasm32")]
            app.init_non_send_resource::<NetworkConnection<ClientConnection>>();

            #[cfg(not(target_arch = "wasm32"))]
            app.init_resource::<NetworkConnection<ClientConnection>>();

            if is_local_server {
                #[cfg(not(target_arch = "wasm32"))]
                app.init_resource::<NetworkConnection<ServerConnection>>();
            }
        }else if is_dedicated_server {
            #[cfg(not(target_arch = "wasm32"))]
            app.init_resource::<NetworkConnection<ServerConnection>>();
        }
    }
}

impl ClientConnection {
    pub fn create_connection(is_authentication_connection: bool, settings: Box<dyn ClientSettingsPort>) -> Option<Self> {
        let mut port = settings.create_port();

        if !port.as_main_port(){
            drop(port);
            return None
        }

        Some(ClientConnection{
            main_port: Some(port),
            secondary_ports: HashMap::new(),
            client_connection_shared_values: ClientConnectionSharedValues{
                runtime: Some(Runtime::new().unwrap()),
            },
            is_authentication_connection
        })
    }

    pub fn create_secondary_port(&mut self, port_id: u32, settings: Box<dyn ClientSettingsPort>)  {
        let port = settings.create_port();

        self.secondary_ports.insert(port_id, port);
    }

    pub fn get_port(&mut self, port_id: u32) -> Option<&mut Box<dyn ClientPort>>{
        if port_id == 0 {
            self.main_port.as_mut()
        }else {
            self.secondary_ports.get_mut(&port_id)
        }
    }
    
    pub fn is_authentication_connection(&self) -> bool{
        self.is_authentication_connection
    }

    pub fn get_port_split(&mut self, port_id: &u32) -> (Option<&mut Box<dyn ClientPort>>, &mut ClientConnectionSharedValues) {
        let port = if *port_id == 0 {
            self.main_port.as_mut()
        } else {
            self.secondary_ports.get_mut(&port_id)
        };

        (port, &mut self.client_connection_shared_values)
    }

    pub fn get_ports_amount(&self) -> u32 {
        self.secondary_ports.len() as u32
    }
    
    pub fn send_server_message<T: MessageTrait>(&mut self, message: Box<dyn MessageTrait>, port_id: u32, send_message_args: Option<Box<dyn SendMessageArgs>>){
        if let (Some(port),client_connection_shared_values) = self.get_port_split(&port_id){
            let type_id = TypeId::of::<T>();
            let registry = MESSAGE_REGISTRY_TYPE_ID.read().unwrap();
            
            port.send_message_for_server(client_connection_shared_values,message.as_ref(),*registry.get(&type_id).unwrap(), send_message_args);
        }
    }
}

impl ServerConnection {
    pub fn create_connection(max_connections: i32, disconnect_peer_if_full: bool, settings: Box<dyn ServerSettingsPort>, is_authentication_connection: bool) -> Option<Self> {
        let mut port = settings.create_port();

        if !port.as_main_port(){
            drop(port);
            return None
        }

        let mut semaphore: Option<Arc<Semaphore>> = None;

        if max_connections > 0 {
            semaphore = Some(Arc::new(Semaphore::new(max_connections as usize)));
        }

        Some(ServerConnection{
            main_port: Some(port),
            secondary_ports: HashMap::new(),
            server_connection_shared_values: ServerConnectionSharedValues{
                runtime: Some(Runtime::new().unwrap()),
            },
            peers_connected: HashMap::new(),
            max_connections,
            disconnect_peer_if_full,
            semaphore,
            is_authentication_connection
        })
    }

    pub fn create_secondary_port(&mut self, port_id: u32, settings: Box<dyn ServerSettingsPort>)  {
        let port = settings.create_port();

        self.secondary_ports.insert(port_id, port);
    }
    
    pub fn disconnect_peer(&mut self, season_uuid: &Uuid, port_id: u32){
        if let Some(port) = self.get_port(port_id){
            port.disconnect_peer(season_uuid);
        }
    }

    pub fn get_ports_amount(&self) -> u32 {
        self.secondary_ports.len() as u32
    }

    pub fn is_peer_connected(&self, season_uuid: &Uuid) -> (bool,bool) {
        let main_port = &self.main_port;

        if let Some(main_port) = main_port {
            main_port.is_peer_connected(season_uuid)
        }else {
            (false, false)
        }
    }

    pub fn get_port(&mut self, port_id: u32) -> Option<&mut Box<dyn ServerPort>>{
        if port_id == 0 {
            self.main_port.as_mut()
        }else {
            self.secondary_ports.get_mut(&port_id)
        }
    }

    pub fn get_port_split(&mut self, port_id: &u32) -> (Option<&mut Box<dyn ServerPort>>, &mut ServerConnectionSharedValues) {
        let port = if *port_id == 0 {
            self.main_port.as_mut()
        } else {
            self.secondary_ports.get_mut(&port_id)
        };

        (port, &mut self.server_connection_shared_values)
    }

    pub fn semaphore(&self) -> Option<Arc<Semaphore>> {
        self.semaphore.as_ref().map(Arc::clone)
    }
    pub fn is_authentication_connection(&self) -> bool{
        self.is_authentication_connection
    }

    pub fn disconnect_peer_if_full(&self) -> bool {
        self.disconnect_peer_if_full
    }
    pub fn send_peer_message<T: MessageTrait>(&mut self, message: Box<dyn MessageTrait>, port_id: u32, season_uuid: &Uuid, send_message_args: Option<Box<dyn SendMessageArgs>>){
        if let (Some(port),server_connection_shared_values ) = self.get_port_split(&port_id){
            let type_id = TypeId::of::<T>();
            let registry = MESSAGE_REGISTRY_TYPE_ID.read().unwrap();
            
            port.send_message_for_peer(server_connection_shared_values, season_uuid, message.as_ref(),*registry.get(&type_id).unwrap(), send_message_args);
        }
    }
}

impl ServerConnectionSharedValues {
    pub fn get_runtime(&self) -> Option<&Runtime>{
        self.runtime.as_ref()
    }
}

impl ClientConnectionSharedValues {
    pub fn get_runtime(&self) -> Option<&Runtime>{
        self.runtime.as_ref()
    }
}

impl NetworkConnection<ServerConnection> {
    pub fn start_connection(&mut self, connection_id: u32, max_connections: i32, disconnect_peer_if_full: bool, settings: Box<dyn ServerSettingsPort>, is_authentication_connection: bool){
        if self.0.contains_key(&connection_id){
            return;
        }

        let new_server_connection = ServerConnection::create_connection(max_connections, disconnect_peer_if_full, settings, is_authentication_connection);

        if new_server_connection.is_none() {
            return;
        }

        self.0.insert(connection_id, new_server_connection.unwrap());
    }

    pub fn create_secondary_port(&mut self, connection_id: u32, port_id: u32, settings: Box<dyn ServerSettingsPort>){
        if let Some(server_connection) = self.0.get_mut(&connection_id){
            server_connection.create_secondary_port(port_id, settings);
        }
    }
}

impl NetworkConnection<ClientConnection> {
    pub fn start_connection(&mut self, connection_id: u32, is_authentication_connection: bool, settings: Box<dyn ClientSettingsPort>){
        if self.0.contains_key(&connection_id){
            return;
        }

        let new_client_connection = ClientConnection::create_connection(is_authentication_connection,settings);

        if new_client_connection.is_none() {
            return;
        }

        self.0.insert(connection_id, new_client_connection.unwrap());
    }

    pub fn create_secondary_port(&mut self, connection_id: u32, port_id: u32, settings: Box<dyn ClientSettingsPort>){
        if let Some(client_connection) = self.0.get_mut(&connection_id){
            client_connection.create_secondary_port(port_id, settings);
        }
    }
}