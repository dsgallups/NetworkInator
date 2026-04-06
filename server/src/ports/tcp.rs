use std::any::Any;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use bevy::log::warn;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use shared::plugins::network::{DefaultNetworkPortSharedInfosServer, PortReliability, ServerPortTrait, ServerSettingsPort};
use std::io::{Error, ErrorKind};
use std::sync::{Arc};
use std::time::{Instant};
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;
use shared::plugins::messaging::{MessageInfos, MessageTrait};
use shared::port_systems::read_writer_tcp::{extract_messages_from_buffer, value_from_number, write_from_settings, BytesOptions, OrderOptions};

pub struct TcpServerSettings{
    address: IpAddr,
    port: u16,
    bytes: BytesOptions,
    order: OrderOptions,
    hook_stream: Option<fn(tcp_stream: TcpStream) -> TcpStream>,
}

#[allow(dead_code)]
pub struct PeerConnected{
    peer_id: Option<Uuid>,
    owned_semaphore_permit: Option<OwnedSemaphorePermit>,
    owned_read_half: OwnedReadHalf,
    owned_write_half: Arc<Mutex<OwnedWriteHalf>>,
    socket_addr: SocketAddr,
    internal_buffer: Vec<u8>,
    non_authenticated_time: f32
}

pub struct TcpServerPort{
    tcp_listener: Option<Arc<TcpListener>>,
    settings: TcpServerSettings,

    started: bool,
    starting: bool,
    first_started: bool,
    main_port: bool,

    peers_connected: HashMap<Uuid,PeerConnected>,
    peers_authenticated: HashMap<Uuid,Uuid>,

    tpc_listener_receiver: UnboundedReceiver<Arc<TcpListener>>,
    tcp_listener_sender: Arc<UnboundedSender<Arc<TcpListener>>>,

    connecting_downed_receiver: UnboundedReceiver<(Error,bool)>,
    connecting_downed_sender: Arc<UnboundedSender<(Error,bool)>>,

    peer_connected_receiver: UnboundedReceiver<(TcpStream, SocketAddr, Option<OwnedSemaphorePermit>)>,
    peer_connected_sender: Arc<UnboundedSender<(TcpStream, SocketAddr, Option<OwnedSemaphorePermit>)>>,

    peer_disconnected_receiver: UnboundedReceiver<(Uuid,Option<Uuid>,Error)>,
    peer_disconnected_sender: Arc<UnboundedSender<(Uuid,Option<Uuid>,Error)>>,
}

impl TcpServerSettings {
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;

        self
    }
}

impl Default for TcpServerSettings {
    fn default() -> Self {
        TcpServerSettings {
            address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 8080,
            bytes: BytesOptions::U32,
            order: OrderOptions::LittleEndian,
            hook_stream: None,
        }
    }
}

impl ServerSettingsPort for TcpServerSettings{
    fn create_port(self: Box<Self>) -> Box<dyn ServerPortTrait>{
        let (tcp_listener_sender,tpc_listener_receiver) = unbounded_channel::<Arc<TcpListener>>();
        let (connecting_downed_sender,connecting_downed_receiver) = unbounded_channel::<(Error,bool)>();
        let (peer_connected_sender,peer_connected_receiver) = unbounded_channel::<(TcpStream, SocketAddr, Option<OwnedSemaphorePermit>)>();
        let (peer_disconnected_sender,peer_disconnected_receiver) = unbounded_channel::<(Uuid,Option<Uuid>,Error)>();

        Box::new(TcpServerPort{
            tcp_listener: None,
            settings: *self,

            started: false,
            starting: false,
            first_started: false,
            main_port: false,

            peers_connected: HashMap::new(),
            peers_authenticated: HashMap::new(),

            tpc_listener_receiver,
            tcp_listener_sender: Arc::new(tcp_listener_sender),

            connecting_downed_receiver,
            connecting_downed_sender: Arc::new(connecting_downed_sender),

            peer_connected_receiver,
            peer_connected_sender: Arc::new(peer_connected_sender),

            peer_disconnected_receiver,
            peer_disconnected_sender: Arc::new(peer_disconnected_sender),
        })
    }
}

impl ServerPortTrait for TcpServerPort{
    fn start(&mut self, network_port_shared_infos: &dyn Any){
        if self.started || self.starting { return; }

        self.starting = true;

        if let Some(default_network_port_shared_infos) = network_port_shared_infos.downcast_ref::<DefaultNetworkPortSharedInfosServer>()
        && let Some(runtime) = &default_network_port_shared_infos.get_runtime()
        {
            let main_port = self.main_port;
            let settings = &self.settings;
            let address = (settings.address, settings.port);

            let tcp_listener_sender = Arc::clone(&self.tcp_listener_sender);
            let connecting_downed_sender = Arc::clone(&self.connecting_downed_sender);
            let connection_downed_sender_two = Arc::clone(&self.connecting_downed_sender);
            let peer_connected_sender = Arc::clone(&self.peer_connected_sender);

            let first_started = self.first_started;
            let (start_accepting_connections_sender, mut start_accepting_connections_receiver) = unbounded_channel::<Arc<TcpListener>>();
            let mut tcp_listener: Option<Arc<TcpListener>> = None;
            let semaphore: Option<Arc<Semaphore>> = if main_port && default_network_port_shared_infos.get_semaphore().is_some() {
                if let Some(semaphore) = &default_network_port_shared_infos.get_semaphore() {
                    Some(Arc::clone(semaphore))
                }else {
                    None
                }
            } else {
                None
            };

            runtime.spawn(async move {
                let tcp_listener_future = TcpListener::bind(address);

                tokio::spawn(async move {
                    match tcp_listener_future.await {
                        Ok(tcp_listener) => {
                            let tcp_listener_arc = Arc::new(tcp_listener);

                            if let Err(send_error) = start_accepting_connections_sender.send(Arc::clone(&tcp_listener_arc)) {
                                warn!("Failed to send TCP port connected receiver, error: {}", send_error);
                            }

                            if let Err(send_error) = tcp_listener_sender.send(Arc::clone(&tcp_listener_arc)) {
                                warn!("Failed to send TCP port connected receiver, error: {}", send_error);
                            }
                        }

                        Err(e) => {
                            println!("failed to connect {}", e);

                            if let Err(send_error) = connecting_downed_sender.send((e,first_started)) {
                                warn!("Failed to send TCP port failed to connect, error: {}", send_error);
                            }
                        },
                    }
                });

                loop {
                    match start_accepting_connections_receiver.recv().await {
                        None => {
                            break;
                        }
                        Some(tcp_listener_received) => {
                            tcp_listener = Some(tcp_listener_received);
                        }
                    }
                }

                if tcp_listener.is_some() && let Some(tcp_listener) = &tcp_listener {
                    loop {
                        match tcp_listener.accept().await {
                            Ok((tcp_stream,socket_addr)) => {
                                if let Some(semaphore) = semaphore.clone() {
                                    match semaphore.try_acquire_owned() {
                                        Ok(owned_semaphore_permit) => {
                                            if let Err(send_error) = peer_connected_sender.send((tcp_stream,socket_addr,Some(owned_semaphore_permit))) {
                                                warn!("Failed to send TCP peer connected to port, error: {}", send_error);
                                                break
                                            }
                                        }
                                        Err(_) => {
                                            drop(tcp_stream);
                                        }
                                    }
                                }else if let Err(send_error) = peer_connected_sender.send((tcp_stream,socket_addr,None)) {
                                    warn!("Failed to send TCP peer connected to port, error: {}", send_error);
                                    break
                                }
                            }
                            Err(e) => {
                                if let Err(send_error) = connection_downed_sender_two.send((e,first_started)) {
                                    warn!("Failed to send TCP port connection_aborted, error: {}", send_error);
                                }
                                break
                            }
                        }
                    }
                }
            });
        }
    }

    fn close(&mut self) {
        if let Some(tcp_listener) = self.tcp_listener.take() {
            drop(tcp_listener);
        }
        
        for (_, peers_connected) in self.peers_connected.drain() {
            drop(peers_connected);
        }
    }

    fn started(&mut self) -> (bool,bool) {
        if self.started {
            (true,false)
        }else {
            match self.tpc_listener_receiver.try_recv() {
                Ok(tcp_listener) => {
                    self.started = true;
                    self.starting = false;
                    self.first_started = true;

                    self.tcp_listener = Some(tcp_listener);

                    (true,true)
                },
                Err(_) => {
                    (false,false)
                }
            }
        }
    }

    fn disconnected(&mut self) -> (bool,Option<Error>,bool) {
        match self.connecting_downed_receiver.try_recv() {
            Ok((error, first_started)) => {
                self.started = false;
                self.starting = false;

                if let Some(tcp_listener) = self.tcp_listener.take() {
                    drop(tcp_listener);
                }

                (true,Some(error),first_started)
            },
            Err(_) => {
                (false,None,self.first_started)
            }
        }
    }

    fn get_peers_messages(&mut self) -> HashMap<Uuid, (Vec<Vec<u8>>,Option<Uuid>)> {
        let mut messages_list: HashMap<Uuid, (Vec<Vec<u8>>, Option<Uuid>)> = HashMap::new();
        let settings = &self.settings;

        for (season_uuid, peer_connected) in self.peers_connected.iter_mut() {
            let messages = extract_messages_from_buffer(&mut peer_connected.internal_buffer, &settings.bytes, &settings.order);

            if messages.is_empty() {
                continue;
            }

            messages_list.insert(*season_uuid, (messages,peer_connected.peer_id));
        }

        messages_list
    }

    fn get_port_reliability(&mut self) -> &PortReliability {
        &PortReliability::Reliable
    }

    fn as_main_port(&mut self) -> bool {
        self.main_port = true;

        true
    }

    fn send_message_to_peer(&mut self, message_id: u32, peer_id: Uuid, network_port_shared_infos: &dyn Any, message: &dyn MessageTrait, _send_args: Option<Box<dyn Any>>) {
        if let Some(peer_authenticated) = self.peers_authenticated.get_mut(&peer_id)
        && let Some(peer_connected) = self.peers_connected.get_mut(peer_authenticated)
        && let Some(default_network_port_shared_infos) = network_port_shared_infos.downcast_ref::<DefaultNetworkPortSharedInfosServer>()
        && let Some(runtime) = &default_network_port_shared_infos.get_runtime()
        {
            let message_infos = &MessageInfos{
                message_id,
                message: postcard::to_stdvec(message).unwrap(),
            };

            let buffer = match postcard::to_stdvec(message_infos) {
                Ok(buff) => {buff}
                Err(_) => {
                    warn!("Error to serialize message");
                    return;
                }
            };

            let message_size = buffer.len();
            let owned_write_half = Arc::clone(&peer_connected.owned_write_half);
            let settings = &self.settings;
            let order_options = settings.order;
            let bytes_options = settings.bytes;

            runtime.spawn(async move {
                let mut guard = owned_write_half.lock().await;

                let size_value = value_from_number(message_size as f64, bytes_options);

                if let Err(send_error) = write_from_settings(&mut guard, &size_value, &order_options).await {
                    warn!("Failed to send TCP message server, error: {}", send_error);
                    return;
                }

                if let Err(send_error) = guard.write_all(&buffer).await {
                    warn!("Failed to send TCP all message server, error: {}", send_error);
                }
            });
        }
    }

    fn is_main_port(&self) -> bool {
        self.main_port
    }

    fn get_peers_disconnected(&mut self) -> HashMap<Uuid,(Option<Uuid>, Error)> {
        let mut peers: HashMap<Uuid,(Option<Uuid>, Error)> = HashMap::new();

        while let Ok((season_uuid,peer_id,error)) = self.peer_disconnected_receiver.try_recv() {
            if let Some(peer_connected) = self.peers_connected.remove(&season_uuid){
                drop(peer_connected);
            }

            if let Some(peer_id) = peer_id {
                self.peers_authenticated.remove(&peer_id);
            }

            peers.insert(season_uuid, (peer_id, error));
        }

        let now = Instant::now().elapsed().as_millis_f32();

        self.peers_connected.retain(|season_uuid, peer_connected| {
            if peer_connected.peer_id.is_some() { return true }

            if now - peer_connected.non_authenticated_time >= 120.0 {
                peers.insert(*season_uuid, (peer_connected.peer_id, Error::new(ErrorKind::TimedOut, "Didnt authenticated in time")));
                return false
            };

            true
        });

        peers
    }

    fn peers_connected(&mut self) -> Vec<Uuid> {
        let mut peers: Vec<Uuid> = Vec::new();
        let peers_connected = &mut self.peers_connected;
        let hook_stream = &self.settings.hook_stream;

        while let Ok((mut tcp_stream,socket_addr,owned_semaphore_permit)) = self.peer_connected_receiver.try_recv() {
            let season_uuid = Uuid::new_v4();

            tcp_stream = match hook_stream {
                Some(hook_stream) => {
                    hook_stream(tcp_stream)
                }
                None => {
                    tcp_stream
                }
            };

            let (owned_read_half,owned_write_half) = tcp_stream.into_split();

            peers_connected.insert(season_uuid, PeerConnected{
                peer_id: None,
                owned_semaphore_permit,
                owned_read_half,
                owned_write_half: Arc::new(Mutex::from(owned_write_half)),
                socket_addr,
                internal_buffer: Vec::new(),
                non_authenticated_time: Instant::now().elapsed().as_millis_f32()
            });

            peers.push(season_uuid);
        }
        
        peers
    }

    fn listen_peers(&mut self, _network_port_shared_infos: &dyn Any) {
        for (season_uuid, peer_connected) in self.peers_connected.iter_mut() {
            let mut temp_buf = [0u8; 1024];

            match peer_connected.owned_read_half.try_read(&mut temp_buf) {
                Ok(n) => {
                    peer_connected.internal_buffer.extend_from_slice(&temp_buf[..n]);
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => {
                    if let Err(send_error) = self.peer_disconnected_sender.send((*season_uuid,peer_connected.peer_id,e)) {
                        warn!("Failed to send TCP peer port connection_aborted, error: {}", send_error);
                        continue;
                    }
                },
            }
        }
    }

    fn authenticate_peer(&mut self, current_season_uuid: Uuid, new_peer_id: Uuid, new_season_uuid: Option<Uuid>) {
        if let Some(peer_connected) = self.peers_connected.get_mut(&current_season_uuid) {
            peer_connected.peer_id = Some(new_peer_id);
            
            println!("Peer authenticated");
            
            if let Some(new_season_uuid) = new_season_uuid{
                let peer_connected = self.peers_connected.remove(&current_season_uuid).unwrap();

                self.peers_connected.insert(new_season_uuid,peer_connected);
                self.peers_authenticated.insert(new_peer_id,new_season_uuid);
            }else{
                self.peers_authenticated.insert(new_peer_id,current_season_uuid);
            }
        }
    }

    fn is_season_authenticated(&self, season_uuid: &Uuid) -> bool {
        if let Some(peer_connected) = self.peers_connected.get(season_uuid) {
            return peer_connected.peer_id.is_some()
        }

        false
    }

    fn is_peer_connected(&self, peer_uuid: &Uuid) -> bool {
        self.peers_authenticated.contains_key(peer_uuid)
    }

    fn disconnect_peer_or_season(&mut self, uuid: &Uuid) {
        if let Some(season_uuid) = self.peers_authenticated.get(uuid) {
            if let Some(peer_connected) = self.peers_connected.remove(season_uuid) {
                drop(peer_connected);
            }
        }else if let Some(peer_connected) = self.peers_connected.remove(uuid) {
            drop(peer_connected);
        }
    }
}