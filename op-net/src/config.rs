use ifaces;
use std::net::SocketAddr;
use std::net::SocketAddrV4;
use std::net::IpAddr;
use node::NodeInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]

pub struct Config {
    pub listen_port: u16,
    pub listen_addr: SocketAddr,
    pub multicast_addr: IpAddr,
    pub multicast_ipv4_ifaces: Vec<SocketAddrV4>,
    /// milliseconds
    pub discovery_interval: u64,
    pub discovery_count: u64,
    /// milliseconds
    pub probe_node_timeout: u64,
    pub probe_node_interval: u64,

    /// milliseconds
    pub idle_connection_timeout: u64,
    /// milliseconds
    pub connection_heartbeat_interval: u64,
    /// milliseconds
    pub connection_timeout: u64,

    /// milliseconds
    pub default_ack_timeout: u64,

    pub this_node: NodeInfo,
}

impl Config {
    pub fn multicast_addr(&self) -> &IpAddr {
        &self.multicast_addr
    }
    pub fn listen_addr(&self) -> &SocketAddr {
        &self.listen_addr
    }
    pub fn multicast_ipv4_ifaces(&self) -> &Vec<SocketAddrV4> {
        &self.multicast_ipv4_ifaces
    }
    pub fn listen_port(&self) -> u16 {
        self.listen_port
    }
    pub fn discovery_interval(&self) -> u64 {
        self.discovery_interval
    }
    pub fn discovery_count(&self) -> u64 {
        self.discovery_count
    }
    pub fn probe_node_timeout(&self) -> u64 {
        self.probe_node_timeout
    }
    pub fn probe_node_interval(&self) -> u64 {
        self.probe_node_interval
    }
    pub fn idle_connection_timeout(&self) -> u64 {
        self.idle_connection_timeout
    }
    pub fn connection_heartbeat_interval(&self) -> u64 {
        self.connection_heartbeat_interval
    }
    pub fn connection_timeout(&self) -> u64 {
        self.connection_timeout
    }
    pub fn default_ack_timeout(&self) -> u64 {
        self.default_ack_timeout
    }
    pub fn this_node(&self) -> &NodeInfo {
        &self.this_node
    }
    pub fn this_node_mut(&mut self) -> &mut NodeInfo {
        &mut self.this_node
    }
}

impl Default for Config {
    fn default() -> Self {
        let listen_port = 6666;

        let mut this_host = NodeInfo::default();

        this_host.ipv4.iter_mut().for_each(|addr| { addr.set_port(listen_port) });

        let multicast_ipv4_ifaces = ifaces::get_up_ipv4_multicast().expect("Cannot get multicast interfaces"); // FIXME return error

        Config {
            listen_port,
            listen_addr: "0.0.0.0:6666".parse().unwrap(),
            multicast_addr: "224.6.6.6".parse().unwrap(),
            multicast_ipv4_ifaces,
            discovery_interval: 1000,
            discovery_count: 3,
            probe_node_timeout: 500,
            probe_node_interval: 5000,
            idle_connection_timeout: 2000,
            connection_heartbeat_interval: 2000,
            connection_timeout: 1000,
            default_ack_timeout: 2000,
            this_node: this_host,
        }
    }
}

