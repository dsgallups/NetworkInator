use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use bevy::log::warn;
use bevy::prelude::{Res, Time};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use shared::plugins::network::{PortInfosTrait, PortSendType, SendMessageArgs, ServerConnectionSharedValues, ServerPort, ServerSettingsPort};
use uuid::{Uuid};
use shared::plugins::messaging::{MessageInfos, MessageTrait};
use shared::port_systems::read_writer_tcp::{read_from_settings, read_value_to_usize, value_from_number, write_from_settings, BytesOptions, OrderOptions};

pub struct TcpInfosServer;

pub struct TcpConfigsServer{
    address: IpAddr,
    port: u16,
    hook_stream: Option<fn(tcp_stream: TcpStream) -> TcpStream>,
    bytes: BytesOptions,
    order: OrderOptions
}

#[allow(dead_code)]
pub struct PeerConnected{
    semaphore_permit: Option<OwnedSemaphorePermit>,
    owned_read_half: Arc<Mutex<OwnedReadHalf>>,
    owned_write_half: Arc<Mutex<OwnedWriteHalf>>,
    socketaddr: SocketAddr,
    listening: bool,
    authenticated: Arc<AtomicBool>,
    id: Option<Uuid>
}

#[allow(dead_code)]
pub struct PeerOnQueue{
    owned_read_half: OwnedReadHalf,
    owned_write_half: OwnedWriteHalf,
    socketaddr: SocketAddr
}

pub struct TcpPortServer {
    tcp_listener: Option<Arc<TcpListener>>,
    settings: TcpConfigsServer,

    connected: bool,
    connecting: bool,
    reconnecting: bool,
    accepting_connections: bool,
    main_port: bool,

    connecting_time: f32,

    tpc_listener_receiver: UnboundedReceiver<TcpListener>,
    tcp_listener_sender: Arc<UnboundedSender<TcpListener>>,

    failed_to_connect_receiver: UnboundedReceiver<Error>,
    failed_to_connect_sender: Arc<UnboundedSender<Error>>,

    connecting_downed_receiver: UnboundedReceiver<Error>,
    connecting_downed_sender: Arc<UnboundedSender<Error>>,

    peer_connected_receiver: UnboundedReceiver<(TcpStream, SocketAddr)>,
    peer_connected_sender: Arc<UnboundedSender<(TcpStream, SocketAddr)>>,

    peer_disconnected_receiver: UnboundedReceiver<(Uuid,Option<Uuid>,Error)>,
    peer_disconnected_sender: Arc<UnboundedSender<(Uuid,Option<Uuid>,Error)>>,

    peer_message_receiver: UnboundedReceiver<(Uuid,Option<Uuid>,Vec<u8>)>,
    peer_message_sender: Arc<UnboundedSender<(Uuid,Option<Uuid>,Vec<u8>)>>,

    peers_authenticated: HashMap<Uuid,PeerConnected>,
    anonymous_peers_connected: HashMap<Uuid,PeerConnected>,
    peers_on_queue_to_join: HashMap<Uuid,PeerOnQueue>
}

impl PortInfosTrait for TcpInfosServer {

}

impl Default for TcpConfigsServer {
    fn default() -> Self {
        TcpConfigsServer {
            address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 8080,
            bytes: BytesOptions::U32,
            order: OrderOptions::LittleEndian,
            hook_stream: None,
        }
    }
}

impl TcpConfigsServer {
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;

        self
    }
}

impl ServerSettingsPort for TcpConfigsServer {
    fn create_port(self: Box<Self>) -> Box<dyn ServerPort> {
        let (tcp_listener_sender,tpc_listener_receiver) = unbounded_channel::<TcpListener>();
        let (failed_to_connect_sender,failed_to_connect_receiver) = unbounded_channel::<Error>();
        let (connecting_downed_sender,connecting_downed_receiver) = unbounded_channel::<Error>();
        let (peer_connected_sender,peer_connected_receiver) = unbounded_channel::<(TcpStream, SocketAddr)>();
        let (peer_disconnected_sender,peer_disconnected_receiver) = unbounded_channel::<(Uuid,Option<Uuid>,Error)>();
        let (peer_message_sender,peer_message_receiver) = unbounded_channel::<(Uuid,Option<Uuid>,Vec<u8>)>();

        Box::new(TcpPortServer{
            tcp_listener: None,
            settings: *self,

            connected: false,
            connecting: false,
            reconnecting: false,
            accepting_connections: false,
            main_port: false,

            connecting_time: 0.0f32,

            tpc_listener_receiver,
            tcp_listener_sender: Arc::new(tcp_listener_sender),

            failed_to_connect_receiver,
            failed_to_connect_sender: Arc::new(failed_to_connect_sender),

            connecting_downed_receiver,
            connecting_downed_sender: Arc::new(connecting_downed_sender),

            peer_connected_receiver,
            peer_connected_sender: Arc::new(peer_connected_sender),

            peer_disconnected_receiver,
            peer_disconnected_sender: Arc::new(peer_disconnected_sender),

            peer_message_receiver,
            peer_message_sender: Arc::new(peer_message_sender),

            peers_authenticated: HashMap::new(),
            anonymous_peers_connected: HashMap::new(),
            peers_on_queue_to_join: HashMap::new()
        })
    }
}

impl ServerPort for TcpPortServer{
    fn start(&mut self, server_connection_shared_values: &ServerConnectionSharedValues, time: &Res<Time>) {
        if self.connected || self.connecting { return; }

        self.connecting = true;

        if let Some(runtime) = server_connection_shared_values.get_runtime(){
            self.connecting_time = time.elapsed_secs();

            let settings = &self.settings;
            let address = (settings.address, settings.port);
            let tcp_listener_sender = Arc::clone(&self.tcp_listener_sender);
            let failed_to_connect_sender = Arc::clone(&self.failed_to_connect_sender);

            runtime.spawn(async move {
                let tcp_listener_future = TcpListener::bind(address);

                match tcp_listener_future.await {
                    Ok(tcp_listener) => {
                        if let Err(err) = tcp_listener_sender.send(tcp_listener) {
                            eprintln!("Not possible to send connection tcp_listener: {}", err);
                            return;
                        }
                    }
                    Err(e) => {
                        if let Err(err) = failed_to_connect_sender.send(Error::from(e)) {
                            eprintln!("Not possible to send failed to connect tcp_listener: {}", err);
                            return;
                        }
                    }
                }
            });
        }
    }

    fn as_main_port(&mut self) -> bool {
        self.main_port = true;
        self.main_port
    }

    fn accepting_connections(&mut self, server_connection_shared_values: &ServerConnectionSharedValues) {
        if self.accepting_connections { return; }

        if let Some(runtime) = server_connection_shared_values.get_runtime(){
            if let Some(tcp_listener) = &self.tcp_listener {
                let tcp_listener = Arc::clone(tcp_listener);
                let connecting_downed_sender = Arc::clone(&self.connecting_downed_sender);
                let peer_connected_sender = Arc::clone(&self.peer_connected_sender);

                self.accepting_connections = true;

                runtime.spawn(async move {
                    loop {
                        match tcp_listener.accept().await {
                            Ok((tcp_stream,socketaddr)) => {
                                if let Err(err) = peer_connected_sender.send((tcp_stream, socketaddr)) {
                                    eprintln!("Not possible to send peer connected: {}", err);
                                    return;
                                }
                            },
                            Err(e) => {
                                match e.kind() {
                                    ErrorKind::ConnectionAborted |
                                    ErrorKind::ConnectionReset => {
                                        continue;
                                    }
                                    _ => {
                                        drop(tcp_listener);

                                        if let Err(err) =  connecting_downed_sender.send(Error::from(e)) {
                                            eprintln!("Not possible to send connection down: {}", err);
                                            return;
                                        }

                                        break;
                                    }
                                }
                            }
                        }
                    }
                });
            }
        }
    }

    fn anonymous_peers_accepted(&mut self, mut semaphore: Option<Arc<Semaphore>>, disconnect_peer_if_full: bool) -> Vec<Uuid> {
        let mut accepted_list: Vec<Uuid> = vec![];
        let settings = &self.settings;
        let hook_stream = settings.hook_stream;
        let main_port = self.main_port;

        if !main_port {
            if let Some(semaphore) = semaphore.take() {
                drop(semaphore);
            }
        }

        loop {
            match self.peer_connected_receiver.try_recv() {
                Ok((mut tcp_stream, socketaddr)) => {
                    if let Some(semaphore) = semaphore.clone() {
                        if disconnect_peer_if_full {
                            match semaphore.try_acquire_owned() {
                                Ok(semaphore) => {
                                    let peer_uuid = Uuid::new_v4();

                                    accepted_list.push(peer_uuid);

                                    tcp_stream = match hook_stream {
                                        Some(hook_stream) => {
                                            hook_stream(tcp_stream)
                                        }
                                        None => {
                                            tcp_stream
                                        }
                                    };

                                    let (owned_read_half,owned_write_half) =  tcp_stream.into_split();

                                    self.anonymous_peers_connected.insert(peer_uuid,PeerConnected{
                                        semaphore_permit: Some(semaphore),
                                        owned_read_half: Arc::new(Mutex::new(owned_read_half)),
                                        owned_write_half: Arc::new(Mutex::new(owned_write_half)),
                                        socketaddr,
                                        listening: false,
                                        authenticated: Default::default(),
                                        id: None,
                                    });
                                },
                                Err(_) => {
                                    drop(tcp_stream);
                                }
                            }
                        }else {
                            match semaphore.try_acquire_owned() {
                                Ok(semaphore) => {
                                    let peer_uuid = Uuid::new_v4();

                                    accepted_list.push(peer_uuid);

                                    tcp_stream = match hook_stream {
                                        Some(hook_stream) => {
                                            hook_stream(tcp_stream)
                                        }
                                        None => {
                                            tcp_stream
                                        }
                                    };

                                    let (owned_read_half,owned_write_half) =  tcp_stream.into_split();

                                    self.anonymous_peers_connected.insert(peer_uuid,PeerConnected{
                                        semaphore_permit: Some(semaphore),
                                        owned_read_half: Arc::new(Mutex::new(owned_read_half)),
                                        owned_write_half: Arc::new(Mutex::new(owned_write_half)),
                                        socketaddr,
                                        listening: false,
                                        authenticated: Default::default(),
                                        id: None,
                                    });
                                },
                                Err(_) => {
                                    let peer_uuid = Uuid::new_v4();

                                    accepted_list.push(peer_uuid);

                                    tcp_stream = match hook_stream {
                                        Some(hook_stream) => {
                                            hook_stream(tcp_stream)
                                        }
                                        None => {
                                            tcp_stream
                                        }
                                    };

                                    let (owned_read_half,owned_write_half) =  tcp_stream.into_split();

                                    self.peers_on_queue_to_join.insert(peer_uuid,PeerOnQueue{
                                        owned_read_half,
                                        owned_write_half,
                                        socketaddr,
                                    });
                                }
                            }
                        }
                    }else{
                        let peer_uuid = Uuid::new_v4();

                        accepted_list.push(peer_uuid);

                        tcp_stream = match hook_stream {
                            Some(hook_stream) => {
                                hook_stream(tcp_stream)
                            }
                            None => {
                                tcp_stream
                            }
                        };

                        let (owned_read_half,owned_write_half) =  tcp_stream.into_split();

                        self.anonymous_peers_connected.insert(peer_uuid,PeerConnected{
                            semaphore_permit: None,
                            owned_read_half: Arc::new(Mutex::new(owned_read_half)),
                            owned_write_half: Arc::new(Mutex::new(owned_write_half)),
                            socketaddr,
                            listening: false,
                            authenticated: Default::default(),
                            id: None,
                        });
                    }
                }
                Err(_) => {
                    break;
                }
            }
        }

        accepted_list
    }

    fn reconnect(&mut self, server_connection_shared_values: &ServerConnectionSharedValues, time: &Res<Time>) {
        if self.reconnecting || self.connecting { return; }

        self.connected = false;
        self.reconnecting = true;
        self.accepting_connections = false;

        if let Some(listener) = self.tcp_listener.take(){
            drop(listener)
        }

        self.start(server_connection_shared_values, time);
    }

    fn check_connection_down(&mut self) -> (bool, Option<Error>) {
        match self.connecting_downed_receiver.try_recv() {
            Ok(error) => {
                self.connecting = false;
                self.reconnecting = false;
                self.connected = false;
                self.accepting_connections = false;

                if let Some(listener) = self.tcp_listener.take(){
                    drop(listener)
                }

                (true, Some(error))
            }
            Err(_) => {
                match self.failed_to_connect_receiver.try_recv() {
                    Ok(error) => {
                        self.connecting = false;
                        self.reconnecting = false;
                        self.connected = false;
                        self.accepting_connections = false;

                        if let Some(listener) = self.tcp_listener.take(){
                            drop(listener)
                        }

                        (true, Some(error))
                    }
                    Err(_) => {
                        (false, None)
                    }
                };
                (false,None)
            }
        }
    }

    fn connected(&mut self) -> bool{
        match self.tpc_listener_receiver.try_recv() {
            Ok(tcp_listener) => {
                self.tcp_listener = Some(Arc::new(tcp_listener));
                self.connected = true;
                true
            },
            Err(_) => {
                false
            }
        }
    }

    fn disconnect(&mut self) {
        if let Some(listener) = self.tcp_listener.take() {
            drop(listener);
        }

        self.peers_authenticated.clear();
        self.anonymous_peers_connected.clear();
        self.peers_on_queue_to_join.clear();
    }

    fn get_connecting_status(&self) -> (bool, bool, f32) {
        (self.connecting,self.reconnecting,self.connecting_time)
    }

    fn check_peers_dropped(&mut self) -> HashMap<Uuid,(Option<Uuid>,Error)> {
        let peer_disconnected_receiver = &mut self.peer_disconnected_receiver;
        let mut disconnected_peers: HashMap<Uuid,(Option<Uuid>,Error)> = HashMap::new();

        self.peers_on_queue_to_join.retain(|_, peer_on_queue| {
            let mut buf = [0u8; 1];

            match peer_on_queue.owned_read_half.try_read(&mut buf) {
                Ok(0) => {
                    true
                }
                Ok(_) => {
                    false
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    false
                }
                Err(_) => {
                    true
                }
            }
        });

        loop {
            match peer_disconnected_receiver.try_recv() {
                Ok((season_uuid, peer_id, error)) => {
                    if !peer_id.is_none() {
                        self.anonymous_peers_connected.remove(&season_uuid);
                    }else {
                        self.peers_authenticated.remove(&season_uuid);
                    }

                    disconnected_peers.insert(season_uuid,(peer_id,error));
                }
                Err(_) => {
                    break
                }
            }
        }

        disconnected_peers
    }

    fn check_peers_on_queue_accepted(&mut self, semaphore: Arc<Semaphore>) -> Vec<Uuid> {
        let mut accepted_list: Vec<Uuid> = vec![];
        let mut semaphores_keys: HashMap<Uuid,OwnedSemaphorePermit> = HashMap::new();

        for (uuid,_) in self.peers_on_queue_to_join.iter() {
            match semaphore.clone().try_acquire_owned() {
                Ok(semaphore_permit) => {
                    accepted_list.push(*uuid);
                    semaphores_keys.insert(*uuid, semaphore_permit);
                }
                Err(_) => {

                }
            }
        }

        for (uuid, semaphore_permit) in semaphores_keys.drain() {
            let peer_on_queue = self.peers_on_queue_to_join.remove(&uuid).unwrap();

            self.anonymous_peers_connected.insert(uuid,PeerConnected{
                semaphore_permit: Some(semaphore_permit),
                owned_read_half: Arc::new(Mutex::from(peer_on_queue.owned_read_half)),
                owned_write_half: Arc::new(Mutex::from(peer_on_queue.owned_write_half)),
                socketaddr: peer_on_queue.socketaddr,
                listening: false,
                authenticated: Default::default(),
                id: None,
            });
        }

        accepted_list
    }

    fn start_listening_anonymous_peers(&mut self, server_connection_shared_values: &ServerConnectionSharedValues) {
        if let Some(runtime) = server_connection_shared_values.get_runtime() {
            let settings = &self.settings;
            let bytes_options = settings.bytes;
            let order_options = settings.order;

            for (uuid,peer_connected) in self.anonymous_peers_connected.iter_mut() {
                if peer_connected.listening { continue; }

                peer_connected.listening = true;

                let authenticated = Arc::clone(&peer_connected.authenticated);
                let owned_read_half = Arc::clone(&peer_connected.owned_read_half);
                let season_uuid = *uuid;
                let peer_uuid = peer_connected.id;

                let peer_disconnected_sender = Arc::clone(&self.peer_disconnected_sender);
                let peer_message_sender = Arc::clone(&self.peer_message_sender);

                runtime.spawn(async move {
                    loop {
                        if authenticated.load(Ordering::SeqCst) {
                            break;
                        }

                        let mut guard = owned_read_half.lock().await;

                        if authenticated.load(Ordering::SeqCst) {
                            break;
                        }

                        match read_from_settings(&mut guard, &bytes_options, &order_options).await {
                            Ok(read_value) => {
                                if authenticated.load(Ordering::SeqCst) {
                                    break;
                                }

                                let size = read_value_to_usize(read_value);
                                let mut buf = vec![0u8; size];

                                if let Err(e) = guard.read_exact(&mut buf).await {
                                    warn!("Error trying read the message: {}", e);

                                    if let Err(err) = peer_disconnected_sender.send((season_uuid, peer_uuid, e)) {
                                        eprintln!("Not possible to send connection error: {}", err);
                                        return;
                                    }
                                    break;
                                }

                                if let Err(err) = peer_message_sender.send((season_uuid,peer_uuid,buf)){
                                    eprintln!("Not possible to send peer message: {}", err);
                                }
                            }
                            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                                continue
                            }
                            Err(e) if e.kind() == ErrorKind::UnexpectedEof ||
                                e.kind() == ErrorKind::ConnectionReset => {
                                if authenticated.load(Ordering::SeqCst) {
                                    break;
                                }
                                if let Err(err) = peer_disconnected_sender.send((season_uuid, peer_uuid, e)) {
                                    eprintln!("Not possible to send connection error: {}", err);
                                    return;
                                }
                                break
                            }
                            Err(e) => {
                                if authenticated.load(Ordering::SeqCst) {
                                    break;
                                }
                                if let Err(err) = peer_disconnected_sender.send((season_uuid, peer_uuid, e)) {
                                    eprintln!("Not possible to send connection error: {}", err);
                                    return;
                                }
                                break
                            }
                        }
                    }
                });
            }
        }
    }

    fn start_listening_authenticated_peers(&mut self, server_connection_shared_values: &ServerConnectionSharedValues) {
        if let Some(runtime) = server_connection_shared_values.get_runtime(){
            let settings = &self.settings;
            let bytes_options = settings.bytes;
            let order_options = settings.order;

            for (uuid,peer_connected) in self.peers_authenticated.iter_mut() {
                if peer_connected.listening { continue; }

                peer_connected.listening = true;

                let owned_read_half = Arc::clone(&peer_connected.owned_read_half);
                let season_uuid = *uuid;
                let peer_uuid = peer_connected.id;

                let peer_disconnected_sender = Arc::clone(&self.peer_disconnected_sender);
                let peer_message_sender = Arc::clone(&self.peer_message_sender);

                runtime.spawn(async move {
                    loop {
                        let mut guard = owned_read_half.lock().await;

                        match read_from_settings(&mut guard, &bytes_options, &order_options).await {
                            Ok(read_value) => {
                                let size = read_value_to_usize(read_value);
                                let mut buf = vec![0u8; size];

                                if let Err(e) = guard.read_exact(&mut buf).await {
                                    warn!("Error trying read the message: {}", e);

                                    if let Err(err) = peer_disconnected_sender.send((season_uuid, peer_uuid, e)) {
                                        eprintln!("Not possible to send connection error: {}", err);
                                        return;
                                    }
                                    break;
                                }

                                if let Err(err) = peer_message_sender.send((season_uuid,peer_uuid,buf)){
                                    eprintln!("Not possible to send peer message: {}", err);
                                }
                            }
                            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                                continue
                            }
                            Err(e) if e.kind() == ErrorKind::UnexpectedEof ||
                                e.kind() == ErrorKind::ConnectionReset => {
                                if let Err(err) = peer_disconnected_sender.send((season_uuid, peer_uuid, e)) {
                                    eprintln!("Not possible to send connection error: {}", err);
                                    return;
                                }
                                break
                            }
                            Err(e) => {
                                if let Err(err) = peer_disconnected_sender.send((season_uuid, peer_uuid, e)) {
                                    eprintln!("Not possible to send connection error: {}", err);
                                    return;
                                }
                                break
                            }
                        }
                    }
                });
            }
        }
    }

    fn get_peers_messages(&mut self) -> Vec<(Uuid, Option<Uuid>, Vec<u8>)> {
        let peer_message_receiver = &mut self.peer_message_receiver;
        let mut messages_received: Vec<(Uuid, Option<Uuid>, Vec<u8>)> = Vec::new();

        loop {
            match peer_message_receiver.try_recv() {
                Ok((season_uuid,peer_id,bytes)) => {
                    messages_received.push((season_uuid,peer_id,bytes));
                }
                Err(_) => {
                    break
                }
            }
        }

        messages_received
    }

    fn authenticate_peer(&mut self, season_uuid: Uuid, peer_id: Uuid, new_season_uuid: Option<Uuid>) -> bool {
        let peer_connected = self.anonymous_peers_connected.remove(&season_uuid);

        if let Some(mut peer_connected) = peer_connected {
            peer_connected.id = Some(peer_id);
            peer_connected.authenticated.store(true,Ordering::SeqCst);
            peer_connected.listening = false;

            if let Some(new_season_uuid) = new_season_uuid {
                self.peers_authenticated.insert(new_season_uuid, peer_connected);
            }else {
                self.peers_authenticated.insert(season_uuid, peer_connected);
            }

            return true;
        }

        false
    }

    fn send_message_for_peer(&mut self, server_connection_shared_values: &ServerConnectionSharedValues, season_uuid: &Uuid, message: &dyn MessageTrait, message_id: u32, send_message_args: Option<Box<dyn SendMessageArgs>>) {
        if let Some(_) = self.anonymous_peers_connected.get_mut(&season_uuid) {
            self.send_message_for_anonymous_peer(server_connection_shared_values, season_uuid, message, message_id, send_message_args);
        }else if let Some(_) = self.peers_authenticated.get_mut(&season_uuid) {
            self.send_message_for_authenticated_peer(server_connection_shared_values, season_uuid, message, message_id, send_message_args);
        }
    }

    fn is_peer_connected(&self, season_uuid: &Uuid) -> (bool, bool) {
        if self.anonymous_peers_connected.contains_key(season_uuid) {
            (true,false)
        }else if self.peers_authenticated.contains_key(season_uuid) {
            (true,true)
        }else {
            (false, false)
        }
    }

    fn get_port_infos(&self) -> &dyn PortInfosTrait {
        &TcpInfosServer
    }

    fn disconnect_peer(&mut self, season_uuid: &Uuid) {
        let peer_disconnected_sender = &self.peer_disconnected_sender;

        if let Some(peer_connected) = self.anonymous_peers_connected.remove(season_uuid) {
            if let Err(err) = peer_disconnected_sender.send((*season_uuid, peer_connected.id, Error::new(ErrorKind::Other, "Peer manually disconnected from server"))) {
                eprintln!("Not possible to send peer disconnected : {}", err);
            }
        }
    }

    fn is_main_port(&self) -> bool {
        self.main_port
    }

    fn get_port_send_type(&self) -> &PortSendType {
        &PortSendType::Reliable
    }
}

impl TcpPortServer {
    fn send_message_for_anonymous_peer(&mut self, server_connection_shared_values: &ServerConnectionSharedValues, season_uuid: &Uuid, message: &dyn MessageTrait, message_id: u32, _: Option<Box<dyn SendMessageArgs>>) {
        if let Some(runtime) = server_connection_shared_values.get_runtime(){
            let peer_anonymous = self.anonymous_peers_connected.get_mut(season_uuid);

            if let Some(peer_anonymous) = peer_anonymous {
                let buffer = match postcard::to_stdvec(message) {
                    Ok(buff) => {buff}
                    Err(_) => {
                        warn!("Error to serialize message");
                        return;
                    }
                };
                let message_infos = MessageInfos{
                    message_id,
                    message: buffer,
                };
                let buffer = match postcard::to_stdvec(&message_infos) {
                    Ok(buff) => {buff}
                    Err(_) => {
                        warn!("Error to serialize message");
                        return;
                    }
                };

                let settings = &self.settings;
                let bytes_options = settings.bytes;
                let order_options = settings.order;
                let owned_write_half = Arc::clone(&peer_anonymous.owned_write_half);
                let message_size = buffer.len();

                runtime.spawn(async move {
                    let mut guard = owned_write_half.lock().await;

                    let size_value = value_from_number(message_size as f64, bytes_options);

                    if let Err(_) = write_from_settings(&mut guard, &size_value, &order_options).await {
                        return;
                    }

                    if let Err(_) = guard.write_all(&buffer).await {
                        return;
                    }
                });
            }else {
                return;
            }
        }
    }

    fn send_message_for_authenticated_peer(&mut self, server_connection_shared_values: &ServerConnectionSharedValues, season_uuid: &Uuid, message: &dyn MessageTrait, message_id: u32, _: Option<Box<dyn SendMessageArgs>>) {
        if let Some(runtime) = server_connection_shared_values.get_runtime(){
            let peer_authenticated = self.peers_authenticated.get_mut(season_uuid);

            if let Some(peer_authenticated) = peer_authenticated {
                let buffer = match postcard::to_stdvec(message) {
                    Ok(buff) => {buff}
                    Err(_) => {
                        warn!("Error to serialize message");
                        return;
                    }
                };
                let message_infos = MessageInfos{
                    message_id,
                    message: buffer,
                };
                let buffer = match postcard::to_stdvec(&message_infos) {
                    Ok(buff) => {buff}
                    Err(_) => {
                        warn!("Error to serialize message");
                        return;
                    }
                };

                let settings = &self.settings;
                let bytes_options = settings.bytes;
                let order_options = settings.order;
                let owned_write_half = Arc::clone(&peer_authenticated.owned_write_half);
                let message_size = buffer.len();

                runtime.spawn(async move {
                    let mut guard = owned_write_half.lock().await;

                    let size_value = value_from_number(message_size as f64, bytes_options);

                    if let Err(_) = write_from_settings(&mut guard, &size_value, &order_options).await {
                        return;
                    }

                    if let Err(_) = guard.write_all(&buffer).await {
                        return;
                    }
                });
            }else {
                return;
            }
        }
    }
}
