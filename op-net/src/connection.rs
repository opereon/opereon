use actix;
use actix::prelude::*;

use futures;
use futures::Future;
use futures::sync::oneshot::Sender;

use std;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use super::msgs::*;
use super::server::*;
use super::server;
use super::codec;

use tokio;
use tokio::io::AsyncRead;
use tokio::io::WriteHalf;
use tokio::net::TcpStream;
use tokio_codec::FramedRead;
use std::net::SocketAddr;
use Config;
use core::fmt;


#[derive(Debug)]
enum ConnectionState {
    /// Initial connection state.
    Disconnected,

    /// Connection established, including handshake.
    Connected,

    /// Handshake in progress.
    Connecting(Sender<Result<(), io::Error>>),

    /// Connection closed.
    Closed,

    /// Connection closed abnormally, protocol violation etc.
    Aborted,

    /// Connection awaiting for ProbeResp message
    AwaitingProbeResp(Sender<()>),
}


#[derive(Debug, PartialEq, Eq, Clone)]
enum ConnectionKind {
    /// Connection initiated by current host.
    Outgoing,
    /// Connection initiated by remote host.
    Incoming,
}


pub struct Connection {
    /// Identical with target host id.
    id: NodeId,
    remote_addr: SocketAddr,
    framed: actix::io::FramedWrite<WriteHalf<TcpStream>, codec::JsonCodec<server::NetMessage>>,
    server: ServerRef,
    config: Arc<Config>,
    last_hb: Instant,
    state: ConnectionState,
    kind: ConnectionKind,
}

//impl Drop for Connection {
//    fn drop(&mut self) {
//        debug!("Connection dropped = {:#?}", self);
//    }
//}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Connection")
            .field("id", &self.id().to_string())
            .field("remote_addr", &self.remote_addr().to_string())
            .field("kind", self.kind())
            .finish()
    }
}

impl Connection {
    pub fn id(&self) -> &NodeId {
        &self.id
    }

    pub fn config(&self) -> &Arc<Config> {
        &self.config
    }

    pub fn server(&self) -> &ServerRef {
        &self.server
    }

    pub fn remote_addr(&self) -> &SocketAddr {
        &self.remote_addr
    }

    pub fn is_outgoing(&self) -> bool {
        self.kind == ConnectionKind::Outgoing
    }

    pub fn is_incoming(&self) -> bool {
        self.kind == ConnectionKind::Incoming
    }

    pub fn is_connected(&self) -> bool {
        if let ConnectionState::Connected = self.state {
            return true;
        }
        false
    }

    pub fn is_connecting(&self) -> bool {
        if let ConnectionState::Connecting(_) = self.state {
            return true;
        }
        false
    }

    pub fn is_disconnected(&self) -> bool {
        if let ConnectionState::Disconnected = self.state {
            return true;
        }
        false
    }

    fn kind(&self) -> &ConnectionKind {
        &self.kind
    }

    fn send_net_message(&mut self, msg: server::NetMessage) {
        trace!("Sending message: {:?}", msg);
        // FIXME https://github.com/actix/actix/issues/132
        // should return future
        self.framed.write(msg)
    }

    /// # Returns
    /// Some(Sender<()) if connection state is `AwaitingProbeResp`, `None` elsewhere
    fn take_awaiting_probe_resp(&mut self) -> Option<Sender<()>> {
        let prev_state = std::mem::replace(&mut self.state, ConnectionState::Disconnected);

        if let ConnectionState::AwaitingProbeResp(sender) = prev_state {
            return Some(sender);
        }
        None
    }

    /// # Returns
    /// Some(Sender<Result<(), Error>>) if connection state is `Connecting`, `None` elsewhere
    fn take_connecting_state(&mut self) -> Option<Sender<Result<(), io::Error>>> {
        let prev_state = std::mem::replace(&mut self.state, ConnectionState::Connected);

        if let ConnectionState::Connecting(sender) = prev_state {
            return Some(sender);
        }
        None
    }

    fn send_handshake(&mut self) {
        let this_host = (*self.config().this_node()).clone();
        self.send_net_message(server::NetMessage::Handshake(this_host))
    }
    // FIXME remove later https://github.com/actix/actix/issues/132
    #[allow(dead_code)]
    fn send_ping(&mut self) {
        self.send_net_message(server::NetMessage::Ping)
    }

    fn send_pong(&mut self) {
        self.send_net_message(server::NetMessage::Pong)
    }

    fn send_handshake_ok(&mut self) {
        self.send_net_message(server::NetMessage::HandshakeOk)
    }

    fn send_probe(&mut self) {
        self.send_net_message(server::NetMessage::Probe)
    }

    fn send_probe_resp(&mut self) {
        self.send_net_message(server::NetMessage::ProbeResp)
    }
    // FIXME remove later https://github.com/actix/actix/issues/132
    #[allow(dead_code)]
    fn register_in_server(&mut self, ctx: &mut Context<Connection>) {
        let conn = ConnectionRef::new(self.id().clone(), ctx.address(), self.remote_addr().clone());
        // Register self in server.
        self.server
            .register_connection(conn)
            .into_actor(self)
            .then(|res, conn, ctx| {
                match res {
                    Err(err) => {
                        error!("Connection registration error = {:?}", err);
                        conn.close(ctx)
                    }
                    _ => {}
                }
                actix::fut::ok::<(), (), Connection>(())
            })
            .wait(ctx); // wait for registration in server before processing any other events
    }
    // FIXME remove later https://github.com/actix/actix/issues/132
    #[allow(dead_code)]
    fn hb(&self, ctx: &mut Context<Self>) {
        let hb_interval = self.config().connection_heartbeat_interval();
        ctx.run_later(Duration::from_millis(hb_interval), move |conn, ctx| {
            if Instant::now().duration_since(conn.last_hb) > Duration::from_millis(2 * hb_interval) {
                // heartbeat timed out
                conn.abort(ctx);
                warn!("Connection heartbeat timeout, stopping {:?}", conn);
                return;
            }

            // Outgoing connection is responsible for sending ping packets
            if conn.is_outgoing() {
                conn.send_ping();
            }

            conn.hb(ctx);
        });
    }

    fn connect(&mut self, ctx: &mut Context<Connection>) -> Box<ActorFuture<Item=ConnectionRef, Error=io::Error, Actor=Connection>> {
        let (tx, rx) = futures::sync::oneshot::channel::<Result<(), io::Error>>();

        self.state = ConnectionState::Connecting(tx);

        // This is outgoing connection, send Handshake(Host) message.
        if self.is_outgoing() {
            self.send_handshake();
        }

        let conn = ConnectionRef::new(self.id().clone(), ctx.address(), self.remote_addr().clone());

        let f
        = rx
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Create result sender cancelled"))
            .map(move |_| conn);

        let f
        = tokio::timer::Deadline::new(f, Instant::now() + Duration::from_secs(self.config().connection_timeout()))
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Connection init timeout reached"))
            .into_actor(self)
            .map(|ret, _connection, _ctx| {
                // FIXME https://github.com/actix/actix/issues/132
                // for now do not register in server

                // Register established connection in server and start heartbeat
//                connection.register_in_server(ctx);
//                connection.hb(ctx);
                ret
            });
        Box::new(f)
    }

    fn probe(&mut self) -> Box<Future<Item=(), Error=io::Error>> {
        debug_assert!(self.is_disconnected(), "Probe allowed only on disconnected connection");

        let (tx, rx) = futures::sync::oneshot::channel::<()>();

        self.send_probe();

        self.state = ConnectionState::AwaitingProbeResp(tx);

        let fut
        = tokio::timer::Deadline::new(rx, Instant::now() + Duration::from_secs(self.config().probe_node_timeout()))
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Probe timeout reached"));
        box fut
    }

    fn close(&mut self, ctx: &mut Context<Connection>) {
        self.close_with_state(ConnectionState::Closed, ctx)
    }

    fn abort(&mut self, ctx: &mut Context<Connection>) {
        self.close_with_state(ConnectionState::Aborted, ctx)
    }

    fn close_with_state(&mut self, state: ConnectionState, ctx: &mut Context<Connection>) {
        self.state = state;

        trace!("Closing connection {:?}", self);

        let conn = ConnectionRef::new(self.id().clone(), ctx.address(), self.remote_addr().clone());

        self.server.unregister_connection(conn);
        self.framed.close();
    }
}

impl Actor for Connection {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
// stop non-connected, idle connection
        let timeout = Duration::from_millis(self.config().idle_connection_timeout());
        ctx.run_later(timeout, |conn, ctx| {
            if conn.is_disconnected() {
                warn!("Stopping idle connection : {}", conn.id());
                conn.abort(ctx);
            }
        });
        trace!("Connection started: {:?}", self);
    }
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        trace!("Connection stopped {:?} ", self);
    }
}

impl actix::io::WriteHandler<io::Error> for Connection {}


impl StreamHandler<server::NetMessage, io::Error> for Connection {
    fn handle(&mut self, msg: server::NetMessage, ctx: &mut Self::Context) {
        fn abort_connection(connection: &mut Connection, ctx: &mut Context<Connection>) {
            warn!("Protocol violation, closing connection: {:?}", connection.id().to_string());
            let err = io::Error::new(io::ErrorKind::Other, "Protocol violation");
            notify_connect_result(connection, ctx, Err(err));
            connection.abort(ctx)
        }

        fn notify_connect_result(connection: &mut Connection, ctx: &mut Context<Connection>, res: Result<(), io::Error>) {
            let connect_result = connection.take_connecting_state();

            if res.is_err() {
                connection.state = ConnectionState::Aborted;
            }

            if let Some(sender) = connect_result {
                match sender.send(res) {
                    Ok(_) => {}
                    Err(_err) => {
                        warn!("Cannot send create result in connection {}", connection.id());
                        ctx.stop();
                    }
                }
            }
        }

        trace!("Message received: {:?}", msg);

        match msg {
            server::NetMessage::Handshake(host) => {
                if !self.is_incoming() {
                    warn!("Handshake received by outgoing connection");
                    abort_connection(self, ctx);
                    return;
                }

                if !self.is_disconnected() {
                    warn!("Handshake message received while connection state is {:?}", self.state);
                    abort_connection(self, ctx);
                    return;
                }

                if host.id().is_nil() {
                    warn!("Host id cannot be nil:  {:?}", host);
                    abort_connection(self, ctx);
                    return;
                }
                let (tx, _rx) = futures::sync::oneshot::channel::<Result<(), io::Error>>();

                self.state = ConnectionState::Connecting(tx);


                // Set connection id to target host id
                self.id = host.id().clone();


                // Add connected host to known hosts
                self.server.add_known_nodes(vec![host]);

                self.send_handshake_ok();

                self.state = ConnectionState::Connected;

                notify_connect_result(self, ctx, Ok(()));
            }
            server::NetMessage::HandshakeOk => {
                if !self.is_outgoing() {
                    warn!("HandshakeOk received by incoming connection");
                    abort_connection(self, ctx);
                    return;
                }

                if !self.is_connecting() {
                    warn!("HandshakeOk message received while connection state is {:?}", self.state);
                    abort_connection(self, ctx);
                    return;
                }

                notify_connect_result(self, ctx, Ok(()));
            }
            server::NetMessage::Ping => {
                if !self.is_connected() {
                    warn!("Ping received before connection established: {}", self.id());
                    abort_connection(self, ctx);
                    return;
                }

                if !self.is_incoming() {
                    // ping packets should be received by incoming connection
                    warn!("Ping received by outgoing connection: {}", self.id());
                    abort_connection(self, ctx);
                    return;
                }
                self.last_hb = Instant::now();
                self.send_pong()
            }
            server::NetMessage::Pong => {
                if !self.is_connected() {
                    warn!("Pong received before connection established: {}", self.id());
                    abort_connection(self, ctx);
                    return;
                }
                if !self.is_outgoing() {
                    // Pong packets should be received by outgoing connection
                    warn!("Pong received by incoming connection: {}", self.id());
                    abort_connection(self, ctx);
                    return;
                }

                self.last_hb = Instant::now();
            }
            server::NetMessage::Probe => {
                if !self.is_disconnected() {
                    warn!("Probe allowed only in Disconnected state: {}", self.id());
                    abort_connection(self, ctx);
                    return;
                }
                self.send_probe_resp();
            }
            server::NetMessage::ProbeResp => {
                if let Some(sender) = self.take_awaiting_probe_resp() {
                    if let Err(_err) = sender.send(()) {
                        warn!("ProbeResp received after timeout: {}", self.id());
                    }
                    return;
                }

                warn!("ProbeResp allowed only in AwaitingProbeResp state: {}", self.id());
                abort_connection(self, ctx);
                return;
            }
            server::NetMessage::Message { payload, ack } => {
                if !self.is_connected() {
                    warn!("Message received before connection established: {}", self.id());
                    abort_connection(self, ctx);
                    return;
                }
                self.server().notify_message_received(payload, self.id(), ack);
            }
            server::NetMessage::Ack { payload, ack } => {
                if !self.is_connected() {
                    warn!("Ack received before connection established: {}", self.id());
                    abort_connection(self, ctx);
                    return;
                }
                self.server().notify_ack_received(payload, ack);
            }
        }
    }

    fn error(&mut self, err: io::Error, ctx: &mut Self::Context) -> Running {
        warn!("Connection stream error, stopping = {} {:?}", self.id(), err);
        self.abort(ctx);
        Running::Stop
    }

    fn finished(&mut self, ctx: &mut Self::Context) {
        // connection closed by remote host
        self.close(ctx)
    }
}

impl Handler<SendNetMessage> for Connection {
    type Result = ();

    fn handle(&mut self, msg: SendNetMessage, _ctx: &mut Self::Context) {
        self.send_net_message(msg.msg)
    }
}

impl Handler<ConnectionConnect> for Connection {
    type Result = ResponseActFuture<Connection, ConnectionRef, io::Error>;

    fn handle(&mut self, _msg: ConnectionConnect, ctx: &mut Self::Context) -> Self::Result {
        self.connect(ctx)
    }
}

impl Handler<ConnectionProbe> for Connection {
    type Result = ResponseFuture<(), io::Error>;

    fn handle(&mut self, _msg: ConnectionProbe, _ctx: &mut Self::Context) -> Self::Result {
        self.probe()
    }
}

impl Handler<Close> for Connection {
    type Result = ();

    fn handle(&mut self, _msg: Close, ctx: &mut Self::Context) {
        self.close(ctx);
    }
}

// Use inner box?
#[derive(Clone, Eq, PartialEq)]
pub struct ConnectionRef {
    id: NodeId,
    // this field can be removed
    addr: Addr<Connection>,
    remote_addr: SocketAddr,
}

impl ConnectionRef {
    pub(crate) fn id(&self) -> &NodeId {
        &self.id
    }

    pub(crate) fn remote_addr(&self) -> &SocketAddr {
        &self.remote_addr
    }

    pub(crate) fn new(id: NodeId, addr: Addr<Connection>, remote_addr: SocketAddr) -> ConnectionRef {
        ConnectionRef {
            id,
            addr,
            remote_addr,
        }
    }

    pub(crate) fn create(is_incoming: bool, id: NodeId, stream: TcpStream, server: ServerRef, config: Arc<Config>) -> ConnectionRef {
//        eprintln!("Creating new connection = {}: {:?}", id, stream.peer_addr().unwrap());
        let remote_addr = stream.peer_addr().expect("Cannot get remote address");

        let connection = Connection::create(move |ctx| {
            let kind = if is_incoming {
                ConnectionKind::Incoming
            } else {
                ConnectionKind::Outgoing
            };

//            eprintln!("Creating new {:?} connection : {} on {:?}", kind, id, stream.peer_addr());

            let (r, w) = stream.split();
            ctx.add_stream(FramedRead::new(r, codec::JsonCodec::new()));


            Connection {
                remote_addr,
                framed: actix::io::FramedWrite::new(w, codec::JsonCodec::new(), ctx),
                id,
                server,
                config,
                state: ConnectionState::Disconnected,
                kind,
                last_hb: Instant::now(),
            }
        });
        ConnectionRef::new(id, connection, remote_addr)
    }

    pub(crate) fn connect(&self) -> Request<Connection, ConnectionConnect> {
        self.addr.send(ConnectionConnect)
    }

    pub(crate) fn probe(&self) -> Request<Connection, ConnectionProbe> {
        self.addr.send(ConnectionProbe)
    }

    pub(crate) fn send_net_message(&self, msg: server::NetMessage) {
        self.addr.do_send(SendNetMessage { msg })
    }
    pub(crate) fn close(&self) {
        self.addr.do_send(Close)
    }
}

impl fmt::Debug for ConnectionRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ConnectionRef")
            .field("id", &self.id().to_string())
            .field("remote_addr", &self.remote_addr().to_string())
            .finish()
    }
}

