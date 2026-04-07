use std::any::Any;
use std::io::{Error, ErrorKind};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use bevy::asset::uuid::Uuid;
use bevy::log::warn;
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use crate::shared::plugins::messaging::{MessageInfos, MessageTrait};
use crate::shared::plugins::network::{ClientPortTrait, ClientSettingsPort, DefaultNetworkPortSharedInfosClient, PortReliability};
use crate::shared::port_systems::inject_extract_uuid::inject_uuid;

pub struct UdpClientSettings {
    address: IpAddr,
    port: u16,
    server_port: u16,
    buffer_size: usize,
    hook_udp_socket: Option<fn(tcp_stream: UdpSocket) -> UdpSocket>,
}

pub struct UdpClientPort {
    settings: UdpClientSettings,
    udp_socket: Option<Arc<UdpSocket>>,

    started: bool,
    starting: bool,
    first_started: bool,
    authenticated: bool,

    last_pong_instant: Instant,
    last_ping_instant: Instant,

    udp_socket_receiver: UnboundedReceiver<Arc<UdpSocket>>,
    udp_socket_sender: Arc<UnboundedSender<Arc<UdpSocket>>>,

    connecting_downed_receiver: UnboundedReceiver<(Error,bool)>,
    connecting_downed_sender: Arc<UnboundedSender<(Error,bool)>>,
}

impl UdpClientSettings {
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;

        self
    }

    pub fn with_server_port(mut self, server_port: u16) -> Self {
        self.server_port = server_port;

        self
    }

    pub fn with_buffer_size(mut self, buffer_size: usize) -> Self {
        self.buffer_size = buffer_size;

        self
    }

    pub fn with_hook_udp_socket(mut self, hook: fn(tcp_stream: UdpSocket) -> UdpSocket) -> Self {
        self.hook_udp_socket = Some(hook);

        self
    }
}

impl Default for UdpClientSettings {
    fn default() -> Self {
        UdpClientSettings {
            address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 0,
            hook_udp_socket: None,
            buffer_size: 1024,
            server_port: 8080
        }
    }
}

impl ClientSettingsPort for UdpClientSettings{
    fn create_port(self: Box<Self>) -> Box<dyn ClientPortTrait>{
        let (udp_socket_sender,udp_socket_receiver) = unbounded_channel::<Arc<UdpSocket>>();
        let (connecting_downed_sender,connecting_downed_receiver) = unbounded_channel::<(Error,bool)>();

        Box::new(UdpClientPort{
            settings: *self,
            udp_socket: None,

            started: false,
            starting: false,
            first_started: false,
            authenticated: false,

            last_pong_instant: Instant::now(),
            last_ping_instant: Instant::now(),

            udp_socket_receiver,
            udp_socket_sender: Arc::new(udp_socket_sender),

            connecting_downed_receiver,
            connecting_downed_sender: Arc::new(connecting_downed_sender),
        })
    }
}

impl ClientPortTrait for UdpClientPort {
    fn start(&mut self, network_port_shared_infos: &dyn Any) {
        if self.started || self.starting { return; }

        self.starting = true;

        if let Some(default_network_port_shared_infos) = network_port_shared_infos.downcast_ref::<DefaultNetworkPortSharedInfosClient>()
            && let Some(runtime) = &default_network_port_shared_infos.get_runtime() {

            let settings = &self.settings;
            let address = (settings.address, settings.port);
            let server_address = (settings.address, settings.server_port);
            let hook_udp_socket = settings.hook_udp_socket;
            let udp_socket_sender = Arc::clone(&self.udp_socket_sender);
            let connecting_downed_sender = Arc::clone(&self.connecting_downed_sender);
            let first_started = self.first_started;

            runtime.spawn(async move {
                let udp_socket_future = UdpSocket::bind(address);

                match udp_socket_future.await {
                    Ok(mut udp_socket_new) => {
                        udp_socket_new = match hook_udp_socket {
                            None => {
                                udp_socket_new
                            }
                            Some(hook_udp_socket) => {
                                hook_udp_socket(udp_socket_new)
                            }
                        };

                        if let Err(e) = udp_socket_new.connect(server_address).await
                        && let Err(send_error) = connecting_downed_sender.send((e,first_started))
                        {
                            warn!("Failed to send UDP port failed to connect, error: {}", send_error);
                            return;
                        }

                        let udp_socket_new = Arc::new(udp_socket_new);
                        
                        if let Err(send_error) = udp_socket_sender.send(Arc::clone(&udp_socket_new)) {
                            warn!("Failed to send UDP port connected receiver, error: {}", send_error);
                        }
                    }
                    Err(e) => {
                        if let Err(send_error) = connecting_downed_sender.send((e,first_started)) {
                            warn!("Failed to send UDP port failed to connect, error: {}", send_error);
                        }
                    }
                };
            });
        }
    }

    fn close(&mut self) {
        if let Some(udp_socket) = self.udp_socket.take() {
            drop(udp_socket);
        }
    }

    fn started(&mut self) -> (bool, bool) {
        if self.started {
            (true,false)
        }else {
            match self.udp_socket_receiver.try_recv() {
                Ok(udp_socket) => {
                    self.started = true;
                    self.starting = false;
                    self.first_started = true;

                    self.last_pong_instant = Instant::now();
                    self.last_ping_instant = Instant::now();
                    self.udp_socket = Some(udp_socket);

                    (true,true)
                },
                Err(_) => {
                    (false,false)
                }
            }
        }
    }

    fn disconnected(&mut self) -> (bool, Option<Error>, bool) {
        if self.started && Instant::now().duration_since(self.last_pong_instant) >= Duration::from_secs(120) {
            self.started = false;
            self.starting = false;

            if let Some(udp_socket) = self.udp_socket.take() {
                drop(udp_socket);
            }

            return (true,Some(Error::new(ErrorKind::TimedOut, "Server didnt sent any message in ages, probably disconnected")), self.first_started)
        }

        match self.connecting_downed_receiver.try_recv() {
            Ok((error, first_started)) => {
                self.started = false;
                self.starting = false;

                if let Some(udp_socket) = self.udp_socket.take() {
                    drop(udp_socket);
                }

                (true,Some(error),first_started)
            },
            Err(_) => {
                (false,None,self.first_started)
            }
        }
    }

    fn get_server_messages(&mut self) -> Vec<Vec<u8>> {
        let mut messages = Vec::new();

        if let Some(udp_socket) = &self.udp_socket {
            let mut buf = vec![0u8; self.settings.buffer_size];

            while let Ok(len) = udp_socket.try_recv(&mut buf) {
                messages.push(buf[..len].to_vec());
            }
        }

        messages
    }

    fn get_port_reliability(&mut self) -> &PortReliability {
        &PortReliability::Unreliable
    }

    fn as_main_port(&mut self) -> bool {
        false
    }

    fn send_message_for_server(&mut self, message_id: u32, network_port_shared_infos: &dyn Any, message: &dyn MessageTrait, local_session_uuid: Option<Uuid>, _send_args: Option<Box<dyn Any>>) {
        if let Some(local_session_uuid) = local_session_uuid && let Some(default_network_port_shared_infos) = network_port_shared_infos.downcast_ref::<DefaultNetworkPortSharedInfosClient>()
            && let Some(runtime) = &default_network_port_shared_infos.get_runtime()
            && let Some(udp_socket) = &self.udp_socket
        {
            let message_infos = &MessageInfos{
                message_id,
                message: postcard::to_stdvec(message).unwrap(),
            };

            let mut buffer = match postcard::to_stdvec(message_infos) {
                Ok(buff) => {buff}
                Err(_) => {
                    warn!("Error to serialize message");
                    return;
                }
            };
            
            buffer = inject_uuid(buffer, local_session_uuid);

            let udp_socket = Arc::clone(udp_socket);

            runtime.spawn(async move {
                udp_socket.send(&buffer).await.ok()
            });
        }
    }

    fn is_main_port(&self) -> bool {
        false
    }

    fn authenticate_port(&mut self) {
        self.authenticated = true;
    }

    fn is_port_authenticated(&self) -> bool {
        self.authenticated
    }

    fn ping(&mut self, local_session_uuid: Uuid, network_port_shared_infos: &dyn Any) {
        let now = Instant::now();

        if now.duration_since(self.last_ping_instant) >= Duration::from_secs(10)
            && let Some(default_network_port_shared_infos) = network_port_shared_infos.downcast_ref::<DefaultNetworkPortSharedInfosClient>()
            && let Some(runtime) = &default_network_port_shared_infos.get_runtime()
            && let Some(udp_socket) = &self.udp_socket
        {
            self.last_ping_instant = now;
            let udp_socket = Arc::clone(udp_socket);

            let mut buffer = match postcard::to_stdvec("ping") {
                Ok(buff) => {buff}
                Err(_) => {
                    warn!("Error to serialize message");
                    return;
                }
            };

            buffer = inject_uuid(buffer, local_session_uuid);

            runtime.spawn(async move {
                udp_socket.send(&buffer).await.ok()
            });
        }
    }

    fn pong(&mut self, bytes: &[u8], _network_port_shared_infos: Option<&dyn Any>) {
        if let Ok(msg) = postcard::from_bytes::<String>(bytes)
            && msg == "ping"
        {
            self.last_pong_instant = Instant::now();
        }
    }
}