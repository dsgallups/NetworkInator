use std::any::Any;
use std::io::{Error, ErrorKind};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use bevy::asset::uuid::Uuid;
use bevy::log::warn;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;
use crate::shared::plugins::messaging::{MessageInfos, MessageTrait};
use crate::shared::plugins::network::{ClientPortTrait, ClientSettingsPort, DefaultNetworkPortSharedInfosClient, PortReliability};
use crate::shared::port_systems::read_writer_tcp::{extract_messages_from_buffer, value_from_number, write_from_settings, BytesOptions, OrderOptions};

pub struct TcpClientSettings{
    address: IpAddr,
    port: u16,
    bytes: BytesOptions,
    order: OrderOptions,
    hook_stream: Option<fn(tcp_stream: TcpStream) -> TcpStream>,
    buffer_size: usize,
}

pub struct TcpClientPort{
    settings: TcpClientSettings,

    started: bool,
    starting: bool,
    first_started: bool,
    main_port: bool,
    authenticated: bool,

    internal_buffer: Vec<u8>,

    owned_read_half: Option<OwnedReadHalf>,
    owned_write_half: Option<Arc<Mutex<OwnedWriteHalf>>>,

    tpc_stream_receiver: UnboundedReceiver<TcpStream>,
    tcp_stream_sender: Arc<UnboundedSender<TcpStream>>,

    connecting_downed_receiver: UnboundedReceiver<(Error,bool)>,
    connecting_downed_sender: Arc<UnboundedSender<(Error,bool)>>,
}

impl TcpClientSettings {
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        
        self
    }

    pub fn with_bytes_options(mut self, bytes_options: BytesOptions) -> Self {
        self.bytes = bytes_options;

        self
    }

    pub fn with_order_options(mut self, order_options: OrderOptions) -> Self {
        self.order = order_options;

        self
    }

    pub fn with_hook_stream(mut self, hook_stream: fn(tcp_stream: TcpStream) -> TcpStream) -> Self {
        self.hook_stream = Some(hook_stream);

        self
    }

    pub fn with_buffer_size(mut self, buffer_size: usize) -> Self {
        self.buffer_size = buffer_size;

        self
    }
}

impl Default for TcpClientSettings{
    fn default()->Self{
        TcpClientSettings{
            address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 8080,
            bytes: BytesOptions::U32,
            order: OrderOptions::LittleEndian,
            hook_stream: None,
            buffer_size: 1024
        }
    }
}

impl ClientSettingsPort for TcpClientSettings{
    fn create_port(self: Box<Self>) -> Box<dyn ClientPortTrait> {
        let (tcp_stream_sender,tpc_stream_receiver) = unbounded_channel::<TcpStream>();
        let (connecting_downed_sender,connecting_downed_receiver) = unbounded_channel::<(Error,bool)>();

        Box::new(TcpClientPort{
            settings: *self,

            started: false,
            starting: false,
            first_started: false,
            main_port: false,
            authenticated: false,

            internal_buffer: Vec::new(),

            owned_read_half: None,
            owned_write_half: None,

            tpc_stream_receiver,
            tcp_stream_sender: Arc::new(tcp_stream_sender),

            connecting_downed_receiver,
            connecting_downed_sender: Arc::new(connecting_downed_sender),
        })
    }
}

impl ClientPortTrait for TcpClientPort{
    fn start(&mut self, network_port_shared_infos: &dyn Any) {
        if self.started || self.starting { return; }

        self.starting = true;

        if let Some(default_network_port_shared_infos) = network_port_shared_infos.downcast_ref::<DefaultNetworkPortSharedInfosClient>() && let Some(runtime) = &default_network_port_shared_infos.get_runtime(){
            let settings = &self.settings;
            let address = (settings.address, settings.port);
            let first_started = self.first_started;
            let hook_stream = settings.hook_stream;

            let connecting_downed_sender = Arc::clone(&self.connecting_downed_sender);
            let tcp_stream_sender = Arc::clone(&self.tcp_stream_sender);

            runtime.spawn(async move {
                let tcp_stream_future = TcpStream::connect(address);

                match tcp_stream_future.await {
                    Ok(mut tcp_stream) => {
                        tcp_stream = match hook_stream {
                            Some(hook_stream) => {
                                hook_stream(tcp_stream)
                            }
                            None => {
                                tcp_stream
                            }
                        };

                        if let Err(send_error) = tcp_stream_sender.send(tcp_stream) {
                            warn!("Failed to send TCP client connected, error: {}", send_error);
                        }
                    }
                    Err(e) => {
                        if let Err(send_error) = connecting_downed_sender.send((e,first_started)) {
                            warn!("Failed to send TCP port failed to connect, error: {}", send_error);
                        }
                    },
                }
            });
        }
    }

    fn close(&mut self) {
        if let Some(owned_read_half) = self.owned_read_half.take() {
            drop(owned_read_half);
        }

        if let Some(owned_write_half) = self.owned_write_half.take() {
            drop(owned_write_half);
        }
    }

    fn started(&mut self) -> (bool, bool) {
        if self.started {
            (true,false)
        }else {
            match self.tpc_stream_receiver.try_recv() {
                Ok(tcp_stream) => {
                    self.started = true;
                    self.starting = false;
                    self.first_started = true;

                    let (owned_read_half,write) = tcp_stream.into_split();

                    self.owned_read_half = Some(owned_read_half);
                    self.owned_write_half = Some(Arc::new(Mutex::new(write)));

                    (true,true)
                }
                Err(_) => {
                    (false,false)
                }
            }
        }
    }

    fn disconnected(&mut self) -> (bool, Option<Error>, bool) {
        match self.connecting_downed_receiver.try_recv() {
            Ok((error, first_started)) => {
                self.started = false;
                self.starting = false;

                self.close();

                (true,Some(error),first_started)
            },
            Err(_) => {
                (false,None,self.first_started)
            }
        }
    }

    fn get_server_messages(&mut self) -> Vec<Vec<u8>> {
        let settings = &self.settings;

        extract_messages_from_buffer(&mut self.internal_buffer, &settings.bytes, &settings.order)
    }

    fn get_port_reliability(&mut self) -> &PortReliability {
        &PortReliability::Reliable
    }

    fn as_main_port(&mut self) -> bool {
        self.main_port = true;

        true
    }

    fn send_message_for_server(&mut self, message_id: u32, network_port_shared_infos: &dyn Any, message: &dyn MessageTrait, _local_session_uuid: Option<Uuid>, _send_args: Option<Box<dyn Any>>) {
        if let Some(default_network_port_shared_infos) = network_port_shared_infos.downcast_ref::<DefaultNetworkPortSharedInfosClient>()
        && let Some(runtime) = &default_network_port_shared_infos.get_runtime()
        && let Some(owned_write_half) = &self.owned_write_half
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
            let owned_write_half = Arc::clone(owned_write_half);
            let settings = &self.settings;
            let order_options = settings.order;
            let bytes_options = settings.bytes;

            runtime.spawn(async move {
                let mut guard = owned_write_half.lock().await;

                let size_value = value_from_number(message_size as f64, bytes_options);

                if let Err(send_error) = write_from_settings(&mut guard, &size_value, &order_options).await {
                    warn!("Failed to send TCP message client, error: {}", send_error);
                    return;
                }

                if let Err(send_error) = guard.write_all(&buffer).await {
                    warn!("Failed to send TCP all message client, error: {}", send_error);
                }
            });
        }
    }

    fn is_main_port(&self) -> bool {
        self.main_port
    }

    fn listen_to_server(&mut self, _network_port_shared_infos: &dyn Any) {
        if let Some(owned_read_half) = &self.owned_read_half {
            let buffer_size = &self.settings.buffer_size;
            let mut temp_buf = vec![0u8; *buffer_size];

            match owned_read_half.try_read(&mut temp_buf) {
                Ok(n) => {
                    self.internal_buffer.extend_from_slice(&temp_buf[..n]);
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {

                }
                Err(e) => {
                    if let Err(send_error) = self.connecting_downed_sender.send((e,self.first_started)) {
                        warn!("Failed to send TCP peer port connection_aborted, error: {}", send_error);
                    }
                },
            }
        }
    }

    fn authenticate_port(&mut self) {
        self.authenticated = true;
    }
    
    fn is_port_authenticated(&self) -> bool {
        self.authenticated
    }
}