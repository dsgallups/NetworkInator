use bevy::DefaultPlugins;
use bevy::prelude::{App, Startup};
use client::plugins::network::ClientNetworkPlugin;
use client::ports::tcp::TcpConfigsClient;
use server::plugins::network::ServerNetworkPlugin;
use server::ports::tcp::TcpConfigsServer;
use shared::NetResMut;
use shared::plugins::authentication::AuthenticationPlugin;
use shared::plugins::messaging::MessagingPlugin;
use shared::plugins::network::{ClientConnection, NetworkConnection, NetworkPlugin, ServerConnection};

fn start_connection(
    mut server_network_connection: NetResMut<NetworkConnection<ServerConnection>>,
    mut client_network_connection: NetResMut<NetworkConnection<ClientConnection>>,
) {
    server_network_connection.start_connection(0, 0, true, Box::new(TcpConfigsServer::default()),true);
    server_network_connection.create_secondary_port(0,1,Box::new(TcpConfigsServer::default().with_port(8081)));

    client_network_connection.start_connection(0,true,Box::new(TcpConfigsClient::default()));
    client_network_connection.create_secondary_port(0,1,Box::new(TcpConfigsClient::default().with_port(8081)));
}

fn main() {
    App::new().add_plugins((DefaultPlugins,ClientNetworkPlugin,ServerNetworkPlugin,NetworkPlugin,MessagingPlugin,AuthenticationPlugin)).add_systems(Startup,start_connection).run();
}
