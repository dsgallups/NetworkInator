use std::any::Any;
use std::collections::HashMap;
use bevy::app::App;
use bevy::prelude::{Plugin, Resource};
use tokio::runtime::Runtime;
use std::io::{Error};
use std::net::SocketAddr;
use std::sync::Arc;
use bevy::asset::uuid::Uuid;
use postcard::from_bytes;
use tokio::sync::Semaphore;
use crate::plugins::messaging::{MessageInfos, MessageTrait};

pub struct NetworkPlugin;

#[derive(PartialEq, Eq)]
pub enum PortReliability{
    Reliable,
    Unreliable
}

#[derive(PartialEq, Eq)]
pub enum NetworkType{
    Client,
    DedicatedServer,
    LocalServer
}

#[cfg(target_arch = "wasm32")]
pub trait ServerPortTrait{
    fn start(&mut self, network_port_shared_infos: &dyn Any);
    fn close(&mut self);
    fn started(&mut self) -> (bool,bool);
    fn disconnected(&mut self) -> (bool,Option<Error>,bool);
    fn get_peers_messages(&mut self) -> HashMap<Uuid, (Vec<Vec<u8>>,Option<Uuid>)>;
    fn get_port_reliability(&mut self) -> &PortReliability;
    fn as_main_port(&mut self) -> bool;
    fn send_message_to_peer(&mut self, message_id: u32, peer_id: Uuid, network_port_shared_infos: &dyn Any, message: &dyn MessageTrait, send_args: Option<Box<dyn Any>>);
    fn is_main_port(&self) -> bool;

    fn deserialize_message_infos(&self, vec: Vec<u8>) -> MessageInfos {
        from_bytes::<MessageInfos>(&vec).unwrap()
    }

    fn get_port_infos(&mut self) -> Option<&dyn Any> {
        None
    }

    fn get_peers_disconnected(&mut self) -> HashMap<Uuid,(Option<Uuid>, Error)> {
        HashMap::new()
    }

    fn peers_connected(&mut self) -> Vec<Uuid> {
        Vec::new()
    }

    fn listen_peers(&mut self, _network_port_shared_infos: &dyn Any) {

    }

    fn authenticate_peer(&mut self, _current_season_uuid: Uuid, _new_peer_id: Uuid, _new_season_uuid: Option<Uuid>) {

    }

    fn is_season_authenticated(&self, _season_uuid: &Uuid) -> bool {
        true
    }

    fn is_port_authenticate_able(&self) -> bool {
        true
    }

    fn is_peer_connected(&self, _peer_uuid: &Uuid) -> bool {
        false
    }

    fn get_peer_socket_socket_addr(&self, _peer_uuid: &Uuid) -> Option<SocketAddr> {
        None
    }

    fn disconnect_peer_or_season(&mut self, _uuid: &Uuid) {

    }
}

#[cfg(not(target_arch = "wasm32"))]
pub trait ServerPortTrait: Send + Sync{
    fn start(&mut self, network_port_shared_infos: &dyn Any);
    fn close(&mut self);
    fn started(&mut self) -> (bool,bool);
    fn disconnected(&mut self) -> (bool,Option<Error>,bool);
    fn get_peers_messages(&mut self) -> HashMap<Uuid, (Vec<Vec<u8>>,Option<Uuid>)>;
    fn get_port_reliability(&mut self) -> &PortReliability;
    fn as_main_port(&mut self) -> bool;
    fn send_message_to_peer(&mut self, message_id: u32, peer_id: Uuid, network_port_shared_infos: &dyn Any, message: &dyn MessageTrait, send_args: Option<Box<dyn Any>>);
    fn is_main_port(&self) -> bool;

    fn deserialize_message_infos(&self, vec: Vec<u8>) -> MessageInfos {
        from_bytes::<MessageInfos>(&vec).unwrap()
    }

    fn get_port_infos(&mut self) -> Option<&dyn Any> {
        None
    }
    
    fn get_peers_disconnected(&mut self) -> HashMap<Uuid,(Option<Uuid>, Error)> {
        HashMap::new()
    }
    
    fn peers_connected(&mut self) -> Vec<Uuid> {
        Vec::new()
    }
    
    fn listen_peers(&mut self, _network_port_shared_infos: &dyn Any) {

    }

    fn authenticate_peer(&mut self, _current_season_uuid: Uuid, _new_peer_id: Uuid, _new_season_uuid: Option<Uuid>) {

    }

    fn is_season_authenticated(&self, _season_uuid: &Uuid) -> bool {
        true
    }

    fn is_port_authenticate_able(&self) -> bool {
        true
    }

    fn is_peer_connected(&self, _peer_uuid: &Uuid) -> bool {
        false
    }
    
    fn get_peer_socket_socket_addr(&self, _peer_uuid: &Uuid) -> Option<SocketAddr> {
        None
    }

    fn disconnect_peer_or_season(&mut self, _uuid: &Uuid) {

    }
}

#[cfg(target_arch = "wasm32")]
pub trait ClientPortTrait {
    fn start(&mut self, network_port_shared_infos: &dyn Any);
    fn close(&mut self);
    fn started(&mut self) -> (bool,bool);
    fn disconnected(&mut self) -> (bool,Option<Error>,bool);
    fn get_server_messages(&mut self) -> Vec<Vec<u8>>;
    fn get_port_reliability(&mut self) -> &PortReliability;
    fn as_main_port(&mut self) -> bool;
    fn send_message_for_server(&mut self, message_id: u32, network_port_shared_infos: &dyn Any, message: &dyn MessageTrait, send_args: Option<Box<dyn Any>>);
    fn is_main_port(&self) -> bool;

    fn deserialize_message_infos(&self, vec: Vec<u8>) -> MessageInfos {
        from_bytes::<MessageInfos>(&vec).unwrap()
    }

    fn get_port_infos(&mut self) -> Option<&dyn Any> {
        None
    }

    fn listen_to_server(&mut self, _network_port_shared_infos: &dyn Any) {

    }

    fn authenticate_port(&mut self) {

    }

    fn is_port_authenticated(&self) -> bool {
        true
    }

    fn is_port_authenticate_able(&self) -> bool {
        true
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub trait ClientPortTrait: Send + Sync{
    fn start(&mut self, network_port_shared_infos: &dyn Any);
    fn close(&mut self);
    fn started(&mut self) -> (bool,bool);
    fn disconnected(&mut self) -> (bool,Option<Error>,bool);
    fn get_server_messages(&mut self) -> Vec<Vec<u8>>;
    fn get_port_reliability(&mut self) -> &PortReliability;
    fn as_main_port(&mut self) -> bool;
    fn send_message_for_server(&mut self, message_id: u32, network_port_shared_infos: &dyn Any, message: &dyn MessageTrait, send_args: Option<Box<dyn Any>>);
    fn is_main_port(&self) -> bool;

    fn deserialize_message_infos(&self, vec: Vec<u8>) -> MessageInfos {
        from_bytes::<MessageInfos>(&vec).unwrap()
    }

    fn get_port_infos(&mut self) -> Option<&dyn Any> {
        None
    }

    fn listen_to_server(&mut self, _network_port_shared_infos: &dyn Any) {

    }

    fn authenticate_port(&mut self) {

    }

    fn is_port_authenticated(&self) -> bool {
        true
    }

    fn is_port_authenticate_able(&self) -> bool {
        true
    }
}

pub trait NetworkPortSharedInfos: Any + Send + Sync{
    fn create_infos_server(server_connection: &ServerConnection) -> Box<Self> where Self: Sized;
    fn create_infos_client(client_connection: &ClientConnection) -> Box<Self> where Self: Sized;
}

pub trait ServerSettingsPort{
    fn create_port(self: Box<Self>) -> Box<dyn ServerPortTrait>;
}

pub trait ClientSettingsPort{
    fn create_port(self: Box<Self>) -> Box<dyn ClientPortTrait>;
}

pub struct DefaultNetworkPortSharedInfosServer {
    runtime: Option<Runtime>,
    semaphore: Option<Arc<Semaphore>>,
}

pub struct DefaultNetworkPortSharedInfosClient {
    runtime: Option<Runtime>,
}

#[derive(Default)]
pub struct ServerConnection{
    main_port: Option<Box<dyn ServerPortTrait>>,
    secondary_ports: HashMap<u32, Box<dyn ServerPortTrait>>,
    network_port_shared_infos: Option<Box<dyn NetworkPortSharedInfos>>,
    max_connections: u32,
    authentication_connection: bool
}

#[derive(Default)]
pub struct ClientConnection{
    main_port: Option<Box<dyn ClientPortTrait>>,
    secondary_ports: HashMap<u32, Box<dyn ClientPortTrait>>,
    network_port_shared_infos: Option<Box<dyn NetworkPortSharedInfos>>,
    authentication_connection: bool
}

#[derive(Resource,Default)]
pub struct NetworkConnection<T>(pub HashMap<u32, T>);

#[derive(Resource)]
pub struct CurrentNetworkSides(pub(crate) Vec<NetworkType>);

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

impl NetworkPortSharedInfos for DefaultNetworkPortSharedInfosServer {
    fn create_infos_server(server_connection: &ServerConnection) -> Box<Self> {
        let mut semaphore: Option<Arc<Semaphore>> = None;
        let max_connections = server_connection.max_connections;

        if max_connections > 0 {
            semaphore = Some(Arc::new(Semaphore::new(max_connections as usize)));
        }

        Box::new(DefaultNetworkPortSharedInfosServer {
            runtime: Some(Runtime::new().unwrap()),
            semaphore
        })
    }

    fn create_infos_client(_client_connection: &ClientConnection) -> Box<Self>
    where
        Self: Sized
    {
        panic!("You shouldn't do this on server")
    }
}

impl NetworkPortSharedInfos for DefaultNetworkPortSharedInfosClient {
    fn create_infos_server(_server_connection: &ServerConnection) -> Box<Self> {
        panic!("You shouldn't do this on Client")
    }

    fn create_infos_client(_client_connection: &ClientConnection) -> Box<Self>
    where
        Self: Sized
    {
        Box::new(DefaultNetworkPortSharedInfosClient {
            runtime: Some(Runtime::new().unwrap())
        })
    }
}

impl DefaultNetworkPortSharedInfosServer {
    pub fn get_semaphore(&self) -> &Option<Arc<Semaphore>> {
        &self.semaphore
    }

    pub fn get_runtime(&self) -> &Option<Runtime> {
        &self.runtime
    }
}

impl DefaultNetworkPortSharedInfosClient {
    pub fn get_runtime(&self) -> &Option<Runtime> {
        &self.runtime
    }
}

impl ServerConnection {
    pub fn create_connection(max_connections: u32, settings: Box<dyn ServerSettingsPort>, authentication_connection: bool) -> Option<Self> {
        let mut port = settings.create_port();

        if !port.as_main_port(){
            drop(port);
            return None
        }

        let server_connection = ServerConnection{
            main_port: Some(port),
            secondary_ports: HashMap::new(),
            max_connections,
            network_port_shared_infos: None,
            authentication_connection
        };

        Some(server_connection)
    }

    pub fn close_port(&mut self, port_id: u32) {
        if port_id == 0 {
            self.close();
        }else if let Some(mut port) = self.secondary_ports.remove(&port_id) {
            port.close();
            drop(port);
        }
    }

    pub fn close(&mut self){
        if let Some(mut main_port) = self.main_port.take() {
            main_port.close();
            drop(main_port);
        }

        for (_, mut port) in self.secondary_ports.drain() {
            port.close();
            drop(port);
        }
    }

    pub fn get_port_split(&mut self, port_id: u32) -> (Option<&mut Box<dyn ServerPortTrait>>, Option<&dyn NetworkPortSharedInfos>){
        let port = if port_id == 0 {
            self.main_port.as_mut()
        } else {
            self.secondary_ports.get_mut(&port_id)
        };

        if let Some(network_port_shared_infos) = &self.network_port_shared_infos{
            (port, Some(network_port_shared_infos.as_ref()))
        }else {
            (port, None)
        }
    }

    pub fn get_port(&mut self, port_id: u32) -> Option<&mut Box<dyn ServerPortTrait>> {
        if port_id == 0 {
            self.main_port.as_mut()
        }else {
            self.secondary_ports.get_mut(&port_id)
        }
    }

    pub fn get_secondary_ports(&mut self) -> &mut HashMap<u32, Box<dyn ServerPortTrait>> {
        &mut self.secondary_ports
    }

    pub fn get_max_connections(&self) -> u32{
        self.max_connections
    }

    pub fn get_ports_amount(&self) -> u32 {
        self.secondary_ports.len() as u32
    }

    pub fn is_authentication_connection(&self) -> bool {
        self.authentication_connection
    }

    pub fn disconnect_peer_or_season(&mut self, uuid: &Uuid){
        let ports_amount = self.get_ports_amount();

        for port_id in 0..=ports_amount {
            if let Some(port) = self.get_port(port_id) {
                port.disconnect_peer_or_season(uuid);
            }
        }
    }

    pub fn open_secondary_port(&mut self, settings: Box<dyn ServerSettingsPort>){
        let ports_amount = self.get_ports_amount();
        let port = settings.create_port();

        self.secondary_ports.insert(ports_amount + 1, port);
    }
}

impl ClientConnection {
    pub fn create_connection(settings: Box<dyn ClientSettingsPort>, authentication_connection: bool) -> Option<Self> {
        let mut port = settings.create_port();

        if !port.as_main_port(){
            drop(port);
            return None
        }

        let client_connection = ClientConnection{
            main_port: Some(port),
            secondary_ports: HashMap::new(),
            network_port_shared_infos: None,
            authentication_connection
        };

        Some(client_connection)
    }

    pub fn close_port(&mut self, port_id: u32) {
        if port_id == 0 {
            self.close();
        }else if let Some(mut port) = self.secondary_ports.remove(&port_id) {
            port.close();
            drop(port);
        }
    }

    pub fn close(&mut self){
        if let Some(mut main_port) = self.main_port.take() {
            main_port.close();
            drop(main_port);
        }

        for (_, mut port) in self.secondary_ports.drain() {
            port.close();
            drop(port);
        }
    }

    pub fn get_port_split(&mut self, port_id: u32) -> (Option<&mut Box<dyn ClientPortTrait>>, Option<&dyn NetworkPortSharedInfos>){
        let port = if port_id == 0 {
            self.main_port.as_mut()
        } else {
            self.secondary_ports.get_mut(&port_id)
        };

        if let Some(network_port_shared_infos) = &self.network_port_shared_infos{
            (port, Some(network_port_shared_infos.as_ref()))
        }else {
            (port, None)
        }
    }

    pub fn get_port(&mut self, port_id: u32) -> Option<&mut Box<dyn ClientPortTrait>> {
        if port_id == 0 {
            self.main_port.as_mut()
        }else {
            self.secondary_ports.get_mut(&port_id)
        }
    }

    pub fn get_secondary_ports(&mut self) -> &mut HashMap<u32, Box<dyn ClientPortTrait>> {
        &mut self.secondary_ports
    }

    pub fn get_ports_amount(&self) -> u32 {
        self.secondary_ports.len() as u32
    }

    pub fn is_authentication_connection(&self) -> bool {
        self.authentication_connection
    }

    pub fn open_secondary_port(&mut self, settings: Box<dyn ClientSettingsPort>){
        let ports_amount = self.get_ports_amount();
        let port = settings.create_port();

        self.secondary_ports.insert(ports_amount + 1, port);
    }
}

impl NetworkConnection<ServerConnection> {
    pub fn start_connection<T: NetworkPortSharedInfos>(&mut self, connection_id: u32, max_connections: u32, settings: Box<dyn ServerSettingsPort>, authentication_connection: bool){
        if self.0.contains_key(&connection_id){
            return;
        }

        let new_server_connection = ServerConnection::create_connection(max_connections, settings, authentication_connection);

        if let Some(mut server_connection) = new_server_connection {
            let network_port_shared_infos = T::create_infos_server(&server_connection);

            server_connection.network_port_shared_infos = Some(network_port_shared_infos);

            self.0.insert(connection_id, server_connection);
        }
    }

    pub fn open_secondary_port(&mut self, connection_id: u32, settings: Box<dyn ServerSettingsPort>){
        if let Some(connection) = self.0.get_mut(&connection_id) {
            connection.open_secondary_port(settings);
        }
    }

    pub(crate) fn send_message(&mut self, message_id: u32, connection_id: u32, port_id: u32, message: &dyn MessageTrait, peer_id: Uuid, send_args: Option<Box<dyn Any>>) {
        if let Some(server_connection) = self.0.get_mut(&connection_id) && let (Some(port),Some(network_port_shared_infos)) = server_connection.get_port_split(port_id) {
            port.send_message_to_peer(message_id, peer_id, network_port_shared_infos, message, send_args);
        }
    }

    pub fn close_port(&mut self, connection_id: u32, port_id: u32) {
        if let Some(connection) = self.0.get_mut(&connection_id) {
            connection.close_port(port_id);
        }
    }

    pub fn close_connection(&mut self, connection_id: u32) {
        if let Some(mut server_connection) = self.0.remove(&connection_id){
            server_connection.close();
            drop(server_connection);
        }
    }

    pub fn disconnect_peer_or_season(&mut self, connection_id: u32, uuid: &Uuid) {
        if let Some(connection) = self.0.get_mut(&connection_id){
            connection.disconnect_peer_or_season(uuid);
        }
    }
}

impl NetworkConnection<ClientConnection> {
    pub fn start_connection<T: NetworkPortSharedInfos>(&mut self, connection_id: u32, settings: Box<dyn ClientSettingsPort>, authentication_connection: bool){
        if self.0.contains_key(&connection_id){
            return;
        }

        let new_server_connection = ClientConnection::create_connection(settings, authentication_connection);

        if let Some(mut client_connection) = new_server_connection {
            let network_port_shared_infos = T::create_infos_client(&client_connection);

            client_connection.network_port_shared_infos = Some(network_port_shared_infos);

            self.0.insert(connection_id, client_connection);
        }
    }

    pub fn open_secondary_port(&mut self, connection_id: u32, settings: Box<dyn ClientSettingsPort>){
        if let Some(connection) = self.0.get_mut(&connection_id) {
            connection.open_secondary_port(settings);
        }
    }

    pub(crate) fn send_message_to_server(&mut self, message_id: u32, connection_id: u32, port_id: u32, message: &dyn MessageTrait, send_args: Option<Box<dyn Any>>) {
        if let Some(client_connection) = self.0.get_mut(&connection_id) && let (Some(port),Some(network_port_shared_infos)) = client_connection.get_port_split(port_id) {
            port.send_message_for_server(message_id, network_port_shared_infos, message, send_args);
        }
    }

    pub fn close_port(&mut self, connection_id: u32, port_id: u32) {
        if let Some(connection) = self.0.get_mut(&connection_id) {
            connection.close_port(port_id);
        }
    }

    pub fn close_connection(&mut self, connection_id: u32) {
        if let Some(mut client_connection) = self.0.remove(&connection_id){
            client_connection.close();
            drop(client_connection);
        }
    }
}

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