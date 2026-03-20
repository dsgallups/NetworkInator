use std::io::Error;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use bevy::log::warn;
use bevy::prelude::{Res, Time};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;
use shared::plugins::messaging::{MessageInfos, MessageTrait};
use shared::plugins::network::{ClientConnectionSharedValues, ClientPort, ClientSettingsPort, PortInfosTrait, PortSendType, SendMessageArgs};
use shared::port_systems::read_writer_tcp::{read_from_settings, read_value_to_usize, value_from_number, write_from_settings, BytesOptions, OrderOptions};

pub struct TcpInfosClient;

pub struct TcpConfigsClient{
    address: IpAddr,
    port: u16,
    hook_stream: Option<fn(tcp_stream: TcpStream) -> TcpStream>,
    bytes: BytesOptions,
    order: OrderOptions
}

pub struct TcpPortClient{
    settings: TcpConfigsClient,
    owned_read_half: Option<Arc<Mutex<OwnedReadHalf>>>,
    owned_write_half: Option<Arc<Mutex<OwnedWriteHalf>>>,

    connected: bool,
    connecting: bool,
    reconnecting: bool,
    main_port: bool,
    listening: bool,
    authenticated: bool,

    connecting_time: f32,

    failed_to_connect_receiver: UnboundedReceiver<Error>,
    failed_to_connect_sender: Arc<UnboundedSender<Error>>,

    connecting_downed_receiver: UnboundedReceiver<Error>,
    connecting_downed_sender: Arc<UnboundedSender<Error>>,

    connected_receiver: UnboundedReceiver<TcpStream>,
    connected_sender: Arc<UnboundedSender<TcpStream>>,

    message_from_server_receiver: UnboundedReceiver<Vec<u8>>,
    message_from_server_sender: Arc<UnboundedSender<Vec<u8>>>,
}


impl PortInfosTrait for TcpInfosClient{

}

impl Default for TcpConfigsClient {
    fn default() -> Self {
        TcpConfigsClient {
            address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 8080,
            bytes: BytesOptions::U32,
            order: OrderOptions::LittleEndian,
            hook_stream: None
        }
    }
}

impl TcpConfigsClient{
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;

        self
    }
}

impl ClientSettingsPort for TcpConfigsClient {
    fn create_port(self: Box<Self>) -> Box<dyn ClientPort> {
        let (failed_to_connect_sender,failed_to_connect_receiver) = unbounded_channel::<Error>();
        let (connecting_downed_sender,connecting_downed_receiver) = unbounded_channel::<Error>();
        let (connected_sender,connected_receiver) = unbounded_channel::<TcpStream>();
        let (message_from_server_sender,message_from_server_receiver) = unbounded_channel::<Vec<u8>>();

        Box::new(TcpPortClient{
            settings: *self,
            owned_read_half: None,
            owned_write_half: None,

            connected: false,
            connecting: false,
            reconnecting: false,
            main_port: false,
            listening: false,
            authenticated: false,

            connecting_time: 0.0,

            failed_to_connect_receiver,
            failed_to_connect_sender: Arc::new(failed_to_connect_sender),

            connecting_downed_receiver,
            connecting_downed_sender: Arc::new(connecting_downed_sender),

            connected_receiver,
            connected_sender: Arc::new(connected_sender),

            message_from_server_receiver,
            message_from_server_sender: Arc::new(message_from_server_sender)
        })
    }
}

impl ClientPort for TcpPortClient {
    fn start(&mut self, client_connection_shared_values: &ClientConnectionSharedValues, time: &Res<Time>) {
        if self.connected || self.connecting { return; }

        self.connecting = true;

        if let Some(runtime) = client_connection_shared_values.get_runtime(){
            self.connecting_time = time.elapsed_secs();

            let connected_sender = Arc::clone(&self.connected_sender);
            let failed_to_connect_sender = Arc::clone(&self.failed_to_connect_sender);
            let settings = &self.settings;
            let address = (settings.address, settings.port);

            runtime.spawn(async move {
                let tcp_stream_future = TcpStream::connect(address);

                match tcp_stream_future.await {
                    Ok(tcp_stream) => {
                        if let Err(err) = connected_sender.send(tcp_stream) {
                            eprintln!("Not possible to send connected tcp_stream: {}", err);
                            return;
                        }
                    }
                    Err(e) => {
                        if let Err(err) = failed_to_connect_sender.send(Error::from(e)) {
                            eprintln!("Not possible to send failed to connect tcp_stream: {}", err);
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

    fn reconnect(&mut self, client_connection_shared_values: &ClientConnectionSharedValues, time: &Res<Time>) {
        if self.reconnecting || self.connecting { return; }

        self.connecting = false;
        self.connected = false;
        self.listening = false;
        self.authenticated = false;
        self.reconnecting = true;

        if let Some(owned_read_half) = self.owned_read_half.take() {
            drop(owned_read_half);
        }

        if let Some(owned_write_half) = self.owned_write_half.take() {
            drop(owned_write_half);
        }

        self.start(client_connection_shared_values, time);
    }

    fn check_connection_down(&mut self) -> (bool, Option<Error>) {
        match self.connecting_downed_receiver.try_recv() {
            Ok(error) => {
                self.connecting = false;
                self.reconnecting = false;
                self.connected = false;
                self.listening = false;
                self.authenticated = false;

                if let Some(owned_read_half) = self.owned_read_half.take() {
                    drop(owned_read_half);
                }

                if let Some(owned_write_half) = self.owned_write_half.take() {
                    drop(owned_write_half);
                }

                (true, Some(error))
            }
            Err(_) => {
                match self.failed_to_connect_receiver.try_recv() {
                    Ok(error) => {
                        self.connecting = false;
                        self.reconnecting = false;
                        self.connected = false;
                        self.listening = false;
                        self.authenticated = false;

                        if let Some(owned_read_half) = self.owned_read_half.take() {
                            drop(owned_read_half);
                        }

                        if let Some(owned_write_half) = self.owned_write_half.take() {
                            drop(owned_write_half);
                        }

                        (true, Some(error))
                    }
                    Err(_) => {
                        (false, None)
                    }
                }
            }
        }
    }

    fn connected(&mut self) -> bool {
        match self.connected_receiver.try_recv() {
            Ok(mut tcp_stream) => {
                let hook_stream = &self.settings.hook_stream;

                tcp_stream = match hook_stream {
                    Some(hook_stream) => {
                        hook_stream(tcp_stream)
                    }
                    None => {
                        tcp_stream
                    }
                };

                let (owned_read_half, owned_write_half) = tcp_stream.into_split();

                self.owned_read_half = Some(Arc::new(Mutex::new(owned_read_half)));
                self.owned_write_half = Some(Arc::new(Mutex::new(owned_write_half)));

                self.connected = true;

                true
            },
            Err(_) => {
                false
            }
        }
    }

    fn disconnect(&mut self) {
        if let Some(owned_read_half) = self.owned_read_half.take() {
            drop(owned_read_half)
        }

        if let Some(owned_write_half) = self.owned_write_half.take() {
            drop(owned_write_half)
        }
    }

    fn start_listening(&mut self, client_connection_shared_values: &ClientConnectionSharedValues) {
        if self.listening || !self.connected {
            return;
        }

        self.listening = true;

        if let Some(runtime) = client_connection_shared_values.get_runtime(){
            if let Some(owned_read_half) = &self.owned_read_half{
                let connecting_downed_sender = Arc::clone(&self.connecting_downed_sender);
                let owned_read_half = Arc::clone(owned_read_half);
                let settings = &self.settings;
                let bytes_options = settings.bytes;
                let order_options = settings.order;
                let message_from_server_sender = Arc::clone(&self.message_from_server_sender);

                runtime.spawn(async move {
                    loop {
                        let mut guard  = owned_read_half.lock().await;

                        match read_from_settings(&mut guard, &bytes_options, &order_options).await {
                            Ok(read_value) => {
                                let size = read_value_to_usize(read_value);
                                let mut buf = vec![0u8; size];

                                if let Err(e) = guard.read_exact(&mut buf).await {
                                    warn!("Error ao ler corpo da message: {}", e);

                                    if let Err(err) = connecting_downed_sender.send(e) {
                                        eprintln!("Not possible to send failed to connection down error: {}", err);
                                        return;
                                    }
                                    
                                    break;
                                }

                                if let Err(err) = message_from_server_sender.send(buf) {
                                    eprintln!("Not possible to send message from server: {}", err);
                                    return;
                                }
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                continue
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof ||
                                e.kind() == std::io::ErrorKind::ConnectionReset => {
                                if let Err(err) = connecting_downed_sender.send(e) {
                                    eprintln!("Not possible to send failed to connection down error: {}", err);
                                    return;
                                }
                                break
                            }
                            Err(e) => {
                                if let Err(err) = connecting_downed_sender.send(e) {
                                    eprintln!("Not possible to send failed to connection down error: {}", err);
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

    fn get_connecting_status(&self) -> (bool, bool, f32) {
        (self.connecting,self.reconnecting,self.connecting_time)
    }

    fn get_port_infos(&self) -> &dyn PortInfosTrait {
        &TcpInfosClient
    }

    fn send_message_for_server(&mut self, client_connection_shared_values: &ClientConnectionSharedValues, message: &dyn MessageTrait, message_id: u32, _: Option<Box<dyn SendMessageArgs>>) {
        if let Some(runtime) = client_connection_shared_values.get_runtime(){
            if let Some(owned_write_half) = &self.owned_write_half {
                let owned_write_half = Arc::clone(owned_write_half);
                let settings = &self.settings;
                let bytes_options = settings.bytes;
                let order_options = settings.order;
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
            }
        }
    }

    fn get_server_messages(&mut self) -> Vec<Vec<u8>> {
        let mut messages: Vec<Vec<u8>>  = Vec::new();

        loop {
            match self.message_from_server_receiver.try_recv() {
                Ok(message) => {
                    messages.push(message);
                }
                Err(_) => {
                    break
                }
            }
        }

        messages
    }

    fn is_port_authenticated(&self) -> bool {
        self.authenticated
    }

    fn set_port_authenticated(&mut self) {
        self.authenticated = true;
    }

    fn is_main_port(&self) -> bool {
        self.main_port
    }

    fn get_port_send_type(&self) -> &PortSendType {
        &PortSendType::Reliable
    }
}