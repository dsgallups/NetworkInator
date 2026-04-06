use std::any::{Any, TypeId};
use std::collections::HashMap;
use bevy::app::{App, Plugin};
use bevy::asset::uuid::Uuid;
use bevy::ecs::system::SystemParam;
use bevy::prelude::{Commands, Message, Messages, ResMut, Resource, Update, World};
use bevy::tasks::ConditionalSend;
use erased_serde::{serialize_trait_object, Serialize as ErasedSerialize};
use serde::{Deserialize, Serialize};
use crate::{NetRes, NetResMut};
use crate::plugins::network::{ClientConnection, CurrentNetworkSides, NetworkConnection, NetworkType, ServerConnection};

#[cfg(target_arch = "wasm32")]
type DispatchMessage = Box<dyn Any>;

#[cfg(not(target_arch = "wasm32"))]
type DispatchMessage = Box<dyn Any + Send + Sync>;

pub trait MessageTrait: 'static + ErasedSerialize + ConditionalSend + Sync {
    fn deserialize(data: &[u8]) -> Self where Self: Sized;
    fn as_authentication(&self) -> bool {
        false
    }
}

serialize_trait_object!(MessageTrait);

pub struct MessagingPlugin;

pub struct MessageFunctionsServer{
    deserialize: fn(&[u8]) -> DispatchMessage,
    dispatch_message: fn(world: &mut World, message: DispatchMessage, connection_id: u32, port_id: u32, peer_uuid: Option<Uuid>, session_id: Uuid),
}

pub struct MessageFunctionsClient{
    deserialize: fn(&[u8]) -> DispatchMessage,
    dispatch_message: fn(world: &mut World, message: DispatchMessage, connection_id: u32, port_id: u32)
}

pub trait MessageTraitPlugin{
    fn register_message<T: MessageTrait>(&mut self);
}

#[derive(Serialize,Deserialize)]
pub struct MessageInfos {
    pub message_id: u32,
    pub message: Vec<u8>,
}

#[derive(Resource, Default)]
pub struct MessagesRegistryClient(u32, HashMap<u32, MessageFunctionsClient>, HashMap<TypeId, u32>);

#[derive(Resource, Default)]
pub struct MessagesRegistryServer(u32, HashMap<u32, MessageFunctionsServer>, HashMap<TypeId, u32>);

#[derive(SystemParam)]
pub struct ServerConnectionParams<'w> {
    messages_registry: NetRes<'w, MessagesRegistryServer>,
    connection: ResMut<'w, NetworkConnection<ServerConnection>>
}

#[derive(SystemParam)]
pub struct ClientConnectionParams<'w> {
    messages_registry: NetRes<'w, MessagesRegistryClient>,
    connection: ResMut<'w, NetworkConnection<ClientConnection>>
}

#[derive(Message)]
pub struct MessageReceivedFromPeer<T: MessageTrait>{
    pub message: T,
    pub peer_uuid: Uuid,
    pub session_uuid: Uuid,
    pub port_id: u32,
    pub connection_id: u32
}

#[derive(Message)]
pub struct MessageReceivedFromAnonymousPeer<T: MessageTrait>{
    pub message: T,
    pub session_uuid: Uuid,
    pub port_id: u32,
    pub connection_id: u32
}

#[derive(Message)]
pub struct MessageReceivedFromServer<T: MessageTrait>{
    pub message: T,
    pub port_id: u32,
    pub connection_id: u32
}

impl<'w> ServerConnectionParams<'w> {
    pub fn send_message<T: MessageTrait>(&mut self, connection_id: u32, port_id: u32, message: &dyn MessageTrait, peer_id: Uuid, send_args: Option<Box<dyn Any>>){
        let type_id = TypeId::of::<T>();

        if let Some(message_id) = self.messages_registry.2.get(&type_id) {
            self.connection.send_message(*message_id, connection_id, port_id, message, peer_id, send_args);
        }
    }

    pub fn get_connections(&mut self) -> &mut ResMut<'w, NetworkConnection<ServerConnection>> {
        &mut self.connection
    }
}

impl<'w> ClientConnectionParams<'w> {
    pub fn send_message<T: MessageTrait>(&mut self, connection_id: u32, port_id: u32, message: &dyn MessageTrait, send_args: Option<Box<dyn Any>>){
        let type_id = TypeId::of::<T>();

        if let Some(message_id) = self.messages_registry.2.get(&type_id) {
            self.connection.send_message_to_server(*message_id, connection_id, port_id, message, send_args);
        }
    }

    pub fn get_connections(&mut self) -> &mut ResMut<'w, NetworkConnection<ClientConnection>> {
        &mut self.connection
    }
}

impl Plugin for MessagingPlugin {
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
            app.init_resource::<MessagesRegistryClient>();

            app.add_systems(Update,check_messages_from_server);

            if is_local_server {
                app.init_resource::<MessagesRegistryServer>();
                app.add_systems(Update,check_messages_from_client);
            }
        }else if is_dedicated_server {
            app.init_resource::<MessagesRegistryServer>();
            app.add_systems(Update,check_messages_from_client);
        }
    }
}

impl MessageTraitPlugin for App {
    fn register_message<T: MessageTrait>(&mut self) {
        let (is_client, is_local_server, is_dedicated_server) = {
            let world = self.world();
            let sides = world.get_resource::<CurrentNetworkSides>()
                .expect("Insert ServerNetworkPlugin or ClientNetworkPlugin first, if its a LocalServer insert both first");
            (
                sides.0.contains(&NetworkType::Client),
                sides.0.contains(&NetworkType::LocalServer),
                sides.0.contains(&NetworkType::DedicatedServer)
            )
        };

        let mut found_message_client = false;
        let mut found_message_server = false;

        if is_client || is_local_server {
            if self.world().get_resource::<Messages<MessageReceivedFromServer<T>>>().is_none() {
                self.add_message::<MessageReceivedFromServer<T>>();
            }else {
                found_message_client = true;
            }

            if is_local_server {
                if self.world().get_resource::<Messages<MessageReceivedFromPeer<T>>>().is_none() {
                    self.add_message::<MessageReceivedFromPeer<T>>();
                    self.add_message::<MessageReceivedFromAnonymousPeer<T>>();
                }else {
                    found_message_server = true;
                }
            }
        }else if is_dedicated_server {
            if self.world().get_resource::<Messages<MessageReceivedFromPeer<T>>>().is_none() {
                self.add_message::<MessageReceivedFromPeer<T>>();
                self.add_message::<MessageReceivedFromAnonymousPeer<T>>();
            }else {
                found_message_server = true;
            }
        }

        if is_client || is_local_server {
            if !found_message_client {
                let world = self.world_mut();

                let mut msg_registry = world
                    .get_resource_mut::<MessagesRegistryClient>()
                    .expect("MessagesRegistryClient not registered; please add MessagingPlugin first");
                let new_value = msg_registry.0 + 1;
                let type_id = TypeId::of::<T>();

                msg_registry.0 = new_value;

                msg_registry.1.insert(new_value, MessageFunctionsClient{
                    deserialize: deserialize_message::<T>,
                    dispatch_message: dispatch_message_client::<T>,
                });

                msg_registry.2.insert(type_id,new_value);
            }

            if !found_message_server {
                let world = self.world_mut();

                if is_local_server {
                    let mut msg_registry = world
                        .get_resource_mut::<MessagesRegistryServer>()
                        .expect("MessagesRegistryServer not registered; please add MessagingPlugin first");
                    let new_value = msg_registry.0 + 1;
                    let type_id = TypeId::of::<T>();

                    msg_registry.0 = new_value;

                    msg_registry.1.insert(new_value, MessageFunctionsServer{
                        deserialize: deserialize_message::<T>,
                        dispatch_message: dispatch_message_server::<T>,
                    });

                    msg_registry.2.insert(type_id,new_value);
                }
            }
        }else if is_dedicated_server && !found_message_server {
            let world = self.world_mut();

            let mut msg_registry = world
                .get_resource_mut::<MessagesRegistryServer>()
                .expect("MessagesRegistryServer not registered; please add MessagingPlugin first");
            let new_value = msg_registry.0 + 1;
            let type_id = TypeId::of::<T>();

            msg_registry.0 = new_value;

            msg_registry.1.insert(new_value, MessageFunctionsServer{
                deserialize: deserialize_message::<T>,
                dispatch_message: dispatch_message_server::<T>,
            });

            msg_registry.2.insert(type_id,new_value);
        }
    }
}

fn check_messages_from_client(
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
    messages_registry_server: NetRes<MessagesRegistryServer>,
    mut commands: Commands,
){
    for (connection_id,connection) in network_connection.0.iter_mut(){
        if let Some(main_port) = connection.get_port(0){
            for (season_uuid, (messages, peer_uuid)) in main_port.get_peers_messages() {
                for bytes in messages {
                    let message_infos = main_port.deserialize_message_infos(bytes);

                    if let Some(registry) = messages_registry_server.1.get(&message_infos.message_id) {
                        let message = (registry.deserialize)(&message_infos.message);
                        let dispatch = registry.dispatch_message;
                        let connection_id = *connection_id;

                        commands.queue(move |world: &mut World| {
                            dispatch(world, message, connection_id, 0, peer_uuid, season_uuid);
                        })
                    }
                }

            }
        }

        for (port_id,port) in connection.get_secondary_ports().iter_mut() {
            for (season_uuid, (messages, peer_uuid)) in port.get_peers_messages() {
                for bytes in messages {
                    let message_infos = port.deserialize_message_infos(bytes);

                    if let Some(registry) = messages_registry_server.1.get(&message_infos.message_id) {
                        let message = (registry.deserialize)(&message_infos.message);
                        let dispatch = registry.dispatch_message;
                        let connection_id = *connection_id;
                        let port_id = *port_id;

                        commands.queue(move |world: &mut World| {
                            dispatch(world, message, connection_id, port_id, peer_uuid, season_uuid);
                        })
                    }
                }
            }
        }
    }
}

fn check_messages_from_server(
    mut network_connection: NetResMut<NetworkConnection<ClientConnection>>,
    messages_registry_client: NetRes<MessagesRegistryClient>,
    mut commands: Commands,
){
    for (connection_id,connection) in network_connection.0.iter_mut(){
        if let Some(main_port) = connection.get_port(0){
            for bytes in main_port.get_server_messages() {
                let message_infos = main_port.deserialize_message_infos(bytes);

                if let Some(registry) = messages_registry_client.1.get(&message_infos.message_id) {
                    let message = (registry.deserialize)(&message_infos.message);
                    let dispatch = registry.dispatch_message;
                    let connection_id = *connection_id;

                    commands.queue(move |world: &mut World| {
                        dispatch(world, message, connection_id, 0);
                    })
                }
            }
        }

        for (port_id,port) in connection.get_secondary_ports().iter_mut() {
            for bytes in port.get_server_messages() {
                let message_infos = port.deserialize_message_infos(bytes);

                if let Some(registry) = messages_registry_client.1.get(&message_infos.message_id) {
                    let message = (registry.deserialize)(&message_infos.message);
                    let dispatch = registry.dispatch_message;
                    let connection_id = *connection_id;
                    let port_id = *port_id;

                    commands.queue(move |world: &mut World| {
                        dispatch(world, message, connection_id, port_id);
                    })
                }
            }
        }
    }
}

fn deserialize_message<T: MessageTrait>(bytes: &[u8]) -> DispatchMessage {
    let message = T::deserialize(bytes);

    Box::new(message)
}

fn dispatch_message_server<T: MessageTrait>(world: &mut World, message: DispatchMessage, connection_id: u32, port_id: u32, peer_uuid: Option<Uuid>, session_id: Uuid)  {
    let message = match  message.downcast::<T>(){
        Ok(message) => message,
        Err(error) => { println!("Failed to downcast message type: {:?}", error); return; }
    };

    if let Some(peer_uuid) = peer_uuid{
        world.write_message(MessageReceivedFromPeer {
            message: *message,
            peer_uuid,
            session_uuid: session_id,
            port_id,
            connection_id,
        });
    }else {
        world.write_message(MessageReceivedFromAnonymousPeer {
            message: *message,
            session_uuid: session_id,
            port_id,
            connection_id,
        });
    }
}

fn dispatch_message_client<T: MessageTrait>(world: &mut World, message: DispatchMessage, connection_id: u32, port_id: u32)  {
    let message = message.downcast::<T>().expect("Failed to downcast");

    world.write_message(MessageReceivedFromServer{
        message: *message,
        port_id,
        connection_id,
    });
}