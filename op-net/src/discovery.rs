use actix::prelude::*;

use tokio::net::UdpSocket;
use tokio::timer::Delay;
use tokio_udp::UdpFramed;

use futures;
use futures::stream::SplitSink;
use futures::{Stream, Sink};
use futures::Future;
use futures::sync::mpsc::UnboundedSender;
use futures::sync::mpsc::UnboundedReceiver;

use super::server::ServerRef;

use std::io;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::sync::Arc;
use std::time::Instant;
use std::time::Duration;

use codec::JsonCodec;
use node::NodeInfo;
use config::Config;
use msgs::Close;
use msgs::Discover;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DiscoveryPackage {
    kind: PackageKind,
    node: NodeInfo,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub enum PackageKind {
    Request,
    Response,
}


impl DiscoveryPackage {
    pub fn new_request(host: NodeInfo) -> DiscoveryPackage {
        DiscoveryPackage {
            kind: PackageKind::Request,
            node: host,
        }
    }
    pub fn new_response(host: NodeInfo) -> DiscoveryPackage {
        DiscoveryPackage {
            kind: PackageKind::Response,
            node: host,
        }
    }
    pub fn is_request(&self) -> bool {
        self.kind == PackageKind::Request
    }
}

#[derive(Debug)]
pub struct UdpPacket {
    payload: DiscoveryPackage,
    addr: SocketAddr,
}

impl UdpPacket {
    pub fn new(data: DiscoveryPackage, addr: SocketAddr) -> UdpPacket {
        UdpPacket {
            payload: data,
            addr,
        }
    }
}


pub struct Discovery {
    server: Option<ServerRef>,
    udp_sender: UnboundedSender<UdpPacket>,
    this_host: Arc<NodeInfo>,
    config: Arc<Config>,

    /// Temporary field necessary for sending udp packets.
    /// `None` after actor started()
    _receiver: Option<UnboundedReceiver<UdpPacket>>,
    /// Temporary field necessary for sending udp packets.
    /// `None` after actor started()
    _package_sink: Option<SplitSink<UdpFramed<JsonCodec<DiscoveryPackage>>>>,
}


impl Actor for Discovery {
    type Context = Context<Discovery>;

    fn started(&mut self, ctx: &mut <Self as Actor>::Context) {
        info!("Discovery: Started");

        self.start_sender_loop(ctx);
    }

    fn stopped(&mut self, _ctx: &mut <Self as Actor>::Context) {
        // drop server ref to avoid memory leak
        self.server = None;
        info!("Discovery: Stopped");
    }
}


impl Discovery {
    pub fn this_host(&self) -> &NodeInfo {
        &(*self.this_host)
    }
    pub fn server(&self) -> &ServerRef {
        let r = self.server.as_ref();
        // This is safe since self.server == None only after discovery stopped()
        r.unwrap()
    }
    pub fn config(&self) -> &Arc<Config> {
        &self.config
    }
    pub fn start(sock: UdpSocket, config: Arc<Config>, server: ServerRef) -> DiscoveryRef {
        // TODO test discovery with large number of hosts to check if there is no problems with mailbox capacity
        let this_host = config.this_node().clone();

        let framed = UdpFramed::new(sock, JsonCodec::new());
        let (sink, stream) = framed.split();

        let addr = Discovery::create(|ctx| {
            ctx.add_stream(stream.map(|(payload, addr)| UdpPacket { payload, addr }));

            let (sender, receiver) = futures::sync::mpsc::unbounded::<UdpPacket>();

            Discovery {
                server: Some(server),
                udp_sender: sender,
                this_host: Arc::new(this_host),
                config,

                _receiver: Some(receiver),
                _package_sink: Some(sink),
            }
        });

        DiscoveryRef(addr)
    }
    pub fn send_packet(&self, packet: UdpPacket) {
        // receiver should never be dropped
        self.udp_sender.unbounded_send(packet).unwrap();
    }

    fn start_sender_loop(&mut self, ctx: &mut Context<Self>) {
        let receiver = self._receiver.take().unwrap();
        let sink = self._package_sink.take().unwrap();

        // this is necessary because sink.send consumes self.
        let sink = Arc::new(Mutex::new(Some(sink)));

        let fut = receiver
            .for_each(move |packet| {
                let tmp_sink = sink.clone();

                let s = tmp_sink.lock().unwrap().take().unwrap();

                s.send((packet.payload, packet.addr))
                    .and_then(|sink| {
                        sink.flush()
                    })
                    .and_then(move |new_sink| {
                        let sink = tmp_sink.clone();
                        let mut tmp_sink = sink.lock().unwrap();
                        *tmp_sink = Some(new_sink);
                        futures::future::ok(())
                    })
                    .then(|res| {
                        // don't stop receiver loop on error
                        match res {
                            Ok(_) => {}
                            Err(err) => {
                                warn!("Sending udp packet error = {:?}", err);
                            }
                        }
                        Ok(())
                    })
            })
            .into_actor(self)
            .then(|_res, _discovery, ctx| {
                // We dont care about cause, just stop actor
                ctx.stop();
                actix::fut::ok(())
            });

        ctx.spawn(fut);
    }

    fn discover(&mut self, _ctx: &mut Context<Discovery>) -> Box<Future<Item=(), Error=io::Error>>{
        let multicast_ipv4 = self.config().multicast_ipv4_ifaces();
        let multicast_sock_addr = SocketAddr::new(*self.config().multicast_addr(), self.config().listen_port());
        let addrs_num = multicast_ipv4.len();
        let discoveries = self.config().discovery_count() as usize;
        let interval = Duration::from_millis(self.config().discovery_interval());

        let mut futs = Vec::with_capacity(addrs_num * discoveries);

        for i in 1..discoveries + 1 {
            for addr in multicast_ipv4.iter() {
                let mut addr: SocketAddr = (*addr).into();
                // Make sure we use random port
                addr.set_port(0);

                // TODO reuse connected UdpSocket
                match UdpSocket::bind(&addr) {
                    Ok(sock) => {
                        let framed = UdpFramed::new(sock, JsonCodec::new());
                        let req_package = DiscoveryPackage::new_request((*self.this_host()).clone());

                        let delay = Delay::new(Instant::now() + (interval * i as u32));

                        let broadcast_fut
                        = delay
                            .then(|res| {
                                if res.is_err() {
                                    warn!("Discovery delay error {:?}", res.unwrap_err());
                                    let err = io::Error::new(io::ErrorKind::Other, "Delay error");
                                    return futures::future::err(err);
                                }
                                futures::future::ok(())
                            })
                            .and_then(move |_| framed.send((req_package, multicast_sock_addr)))
                            .and_then(|framed| framed.flush())
                            .then(|res| {
                                match res {
                                    Ok(_) => {}
                                    Err(err) => {
                                        warn!("Cannot send discovery package = {:?}", err);
                                        return futures::future::err(err);
                                    }
                                }
                                futures::future::ok(())
                            });

                        futs.push(broadcast_fut)
                    }
                    Err(err) => {
                        warn!("Cannot bind udp socket {:?} {:?}", addr, err);
                        return Box::new(futures::future::err(err));
                    }
                }
            }
        }


        let fut = futures::future::collect(futs)
            .map(|_: Vec<()>| ());
        Box::new(fut)
    }

}

impl StreamHandler<UdpPacket, io::Error> for Discovery {
    fn handle(&mut self, packet: UdpPacket, ctx: &mut Context<Self>) {
        let package = packet.payload;
//        println!("Received : {:?} with host {:?}", package.kind, package.host.id().to_string());
        let mut src_addr = packet.addr;
//        eprintln!("src_addr = {:?}", src_addr);
        if package.is_request() && self.this_host().id() != package.node.id() {
            src_addr.set_port(self.config.listen_port());

            let resp_fut = self.server().get_known_nodes()
                .into_actor(self)
                .then(move |res, discovery, _ctx| {
                    match res {
                        Ok(known_nodes) => {
                            let known_nodes = known_nodes.unwrap();

                            for node in known_nodes.into_iter() {
                                let packet = UdpPacket::new(DiscoveryPackage::new_response(node), src_addr);
                                discovery.send_packet(packet);
                            }
                        }
                        Err(err) => {
                            warn!("Mailbox error = {:?}", err);
                        }
                    }
                    actix::fut::ok(())
                });

            ctx.spawn(resp_fut);
        }

        self.server().add_known_nodes(vec![package.node]);
    }

    fn error(&mut self, err: io::Error, _ctx: &mut Context<Self>) -> Running {
        warn!("Udp stream error = {:?}", err);
        Running::Continue
    }
}

impl Handler<Close> for Discovery {
    type Result = ();

    fn handle(&mut self, _msg: Close, ctx: &mut Context<Self>) {
        ctx.stop();
    }
}

impl Handler<Discover> for Discovery {
    type Result = ResponseFuture<(), io::Error>;

    fn handle(&mut self, _: Discover, ctx: &mut Context<Self>) -> Self::Result {
//        println!("Running discovery...");
        self.discover(ctx)
    }
}

pub struct DiscoveryRef(Addr<Discovery>);

impl DiscoveryRef {
    pub fn stop(&self) {
        self.0.do_send(Close)
    }
    pub fn discover(&self) -> Request<Discovery, Discover> {
        self.0.send(Discover)
    }
}

//#[cfg(test)]
//mod tests {
//    use super::*;
//    use ::server::Server;
//    use prelude::PayloadHandler;
//    use server::Payload;
//    use session::Session;
//    use std;
//
//    struct Handler;
//
//    impl PayloadHandler for Handler {
//        fn handle(&self, _msg: Payload, _server: &ServerRef, _session: &Session, _ctx: &mut Context<Session>) {
//            unimplemented!()
//        }
//    }
//
//    #[test]
//    fn discovery() {
//        let config = Config::create().unwrap();
//
//
//        let server = Server::start(config.clone(), Handler).unwrap();
//
//
//        actix::System::run(move || {
//            let _discovery = Discovery::start(config, server).unwrap();
//        });
//    }
//}
