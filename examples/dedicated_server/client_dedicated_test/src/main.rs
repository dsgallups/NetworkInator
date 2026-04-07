use networkinator::shared::plugins::messaging::{ClientConnectionParams, MessageTrait, MessageTraitPlugin, MessagingPlugin};
use bevy::DefaultPlugins;
use bevy::prelude::{App, MessageReader, Startup, Update};
use serde::{Deserialize, Serialize};
use message_pro_macro::ConnectionMessage;
use networkinator::client::ports::tcp::TcpClientSettings;
use networkinator::client::ports::udp::UdpClientSettings;
use networkinator::{NetRes, NetResMut};
use networkinator::client::plugins::network::ClientNetworkPlugin;
use networkinator::shared::plugins::authentication::{AuthenticationPlugin, ClientPortAuthenticated};
use networkinator::shared::plugins::network::{ClientConnection, DefaultNetworkPortSharedInfosClient, LocalSessionUUID, NetworkConnection, NetworkPlugin};

#[derive(Serialize,Deserialize,ConnectionMessage)]
pub struct HiMessage(String);

fn start_connection(
    mut network_connection: NetResMut<NetworkConnection<ClientConnection>>,
) {
    network_connection.start_connection::<DefaultNetworkPortSharedInfosClient>(0, Box::new(TcpClientSettings::default()),true);
    network_connection.open_secondary_port(0, Box::new(UdpClientSettings::default().with_server_port(8070)));
}

fn send_hi_message(
    mut client_port_authenticated: MessageReader<ClientPortAuthenticated>,
    mut client_connection_params: ClientConnectionParams,
    local_session_uuid: NetRes<LocalSessionUUID>,
){
    for event in client_port_authenticated.read() {
        client_connection_params.send_message::<HiMessage>(event.connection_id, event.port_id, &HiMessage("Hi server".parse().unwrap()), local_session_uuid.get_session_uuid(), None);
    }
}

fn main() {
    let mut app = App::new();
    
    app.add_plugins((DefaultPlugins,ClientNetworkPlugin,NetworkPlugin,MessagingPlugin,AuthenticationPlugin));
    app.add_systems(Startup,start_connection);
    app.add_systems(Update,send_hi_message);
    app.register_message::<HiMessage>();
    app.run();
}
