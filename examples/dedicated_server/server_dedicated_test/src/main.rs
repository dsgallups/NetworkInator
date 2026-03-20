use bevy::DefaultPlugins;
use bevy::prelude::{App, Startup};
use server::plugins::network::ServerNetworkPlugin;
use server::ports::tcp::TcpConfigsServer;
use shared::NetResMut;
use shared::plugins::authentication::AuthenticationPlugin;
use shared::plugins::messaging::MessagingPlugin;
use shared::plugins::network::{NetworkConnection, NetworkPlugin, ServerConnection};

fn start_connection(
    mut network_connection: NetResMut<NetworkConnection<ServerConnection>>,
) {
    network_connection.start_connection(0, 0, true, Box::new(TcpConfigsServer::default()),true);
    network_connection.create_secondary_port(0,1,Box::new(TcpConfigsServer::default().with_port(8081)));
}

fn main() {
    App::new().add_plugins((DefaultPlugins,ServerNetworkPlugin,NetworkPlugin,MessagingPlugin,AuthenticationPlugin)).add_systems(Startup,start_connection).run();
}
