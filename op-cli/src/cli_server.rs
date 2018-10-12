use actix::prelude::*;

use std;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use futures::Stream;

use tokio::io::AsyncRead;
use tokio::io::WriteHalf;
use tokio_codec::FramedRead;
use tokio_uds::UnixListener;
use tokio_uds::UnixStream;

use op_exec::{ Outcome, RuntimeError};
use op_net::{JsonCodec, Subscribe};

use commons::{CliMessage, ServerMessage};

use slog;
use daemon::InstanceInfo;

pub struct CliServer {
    cli_msg_sub: Option<Recipient<CliMessageRcvd>>,
    sessions: HashMap<Ssid, Addr<Session>>,
    sock_path: PathBuf,
    logger: slog::Logger,
}

impl Actor for CliServer {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut <Self as Actor>::Context) {
        info!(self.logger, "Cli server started");
    }

    fn stopped(&mut self, _ctx: &mut <Self as Actor>::Context) {
        self.cli_msg_sub = None;
        self.sessions
            .drain()
            .for_each(|(_, sess)| sess.do_send(Stop));
        info!(self.logger, "Cli server stopped");
    }
}

impl Drop for CliServer {
    fn drop(&mut self) {
        if let Err(err) = std::fs::remove_file(&self.sock_path) {
            error!(self.logger, "Cannot remove socket file : {:?}", err)
        }
    }
}

impl CliServer {
    pub fn start(sock_path: &Path, logger: slog::Logger) -> io::Result<CliServerRef> {
        {
            let sock_dir = sock_path.parent();

            if let Some(path) = sock_dir {
                std::fs::create_dir_all(path)?
            }
        }

        let listener = UnixListener::bind(sock_path)?;
        let logger = logger.clone();

        let sock_path = sock_path.to_path_buf();
        let cli = CliServer::create(|ctx| {
            ctx.add_stream(listener.incoming().map(|st| Connection(st)));

            CliServer {
                sessions: HashMap::new(),
                cli_msg_sub: None,
                sock_path,
                logger,
            }
        });
        Ok(CliServerRef(cli))
    }

    fn add_session(&mut self, id: Ssid, addr: Addr<Session>) {
        if self.sessions.insert(id, addr).is_some() {
            warn!(self.logger, "Duplicated session ids = {:?}", id);
        }
    }

    fn remove_session(&mut self, id: &Ssid) {
        if self.sessions.remove(id).is_none() {
            warn!(self.logger, "Removing non-existing session");
        }
    }
}

#[derive(Message)]
struct Connection(pub UnixStream);

#[derive(Message)]
struct Stop;

#[derive(Message)]
struct RegisterSession {
    addr: Addr<Session>,
    id: Ssid,
}

#[derive(Message)]
struct UnregisterSession {
    id: Ssid,
}

#[derive(Message)]
struct SendOutcome (Outcome);

#[derive(Message)]
struct SendErr (RuntimeError);


#[derive(Message)]
struct SendReachableInstances (Vec<InstanceInfo>);

#[derive(Message)]
pub struct CliMessageRcvd {
    pub session: CliSessionRef,
    pub msg: CliMessage,
}

impl CliMessageRcvd {
    fn new(msg: CliMessage, session: CliSessionRef) -> CliMessageRcvd {
        CliMessageRcvd { msg, session }
    }
}

impl Handler<RegisterSession> for CliServer {
    type Result = ();

    fn handle(&mut self, msg: RegisterSession, _ctx: &mut Self::Context) {
        self.add_session(msg.id, msg.addr);
    }
}

impl Handler<UnregisterSession> for CliServer {
    type Result = ();

    fn handle(&mut self, msg: UnregisterSession, _ctx: &mut Self::Context) {
        self.remove_session(&msg.id);
    }
}

impl Handler<Subscribe<CliMessageRcvd>> for CliServer {
    type Result = ();

    fn handle(&mut self, msg: Subscribe<CliMessageRcvd>, _ctx: &mut Context<CliServer>) {
        self.cli_msg_sub = Some(msg.0)
    }
}

impl Handler<Stop> for CliServer {
    type Result = ();

    fn handle(&mut self, _: Stop, ctx: &mut Self::Context) {
        ctx.stop();
    }
}

impl StreamHandler<Connection, io::Error> for CliServer {
    fn handle(&mut self, msg: Connection, ctx: &mut actix::Context<Self>) {
        let addr = ctx.address();
        let logger = self.logger.clone();

        if self.cli_msg_sub.is_none() {
            // there is no subscriber, close connection
            return;
        }

        let cli_msg_sub = self.cli_msg_sub.clone().unwrap();

        // Create new session and register CliMessage stream.
        Session::create(|ctx| {
            let (r, w) = msg.0.split();
            ctx.add_stream(FramedRead::new(r, JsonCodec::new()));

            use std::sync::atomic;
            static COUNTER: atomic::AtomicUsize = atomic::AtomicUsize::new(0);

            Session {
                framed: actix::io::FramedWrite::new(w, JsonCodec::new(), ctx),
                id: COUNTER.fetch_add(1, Ordering::Relaxed),
                server: addr,
                cli_msg_sub,
                logger,
            }
        });
    }

    fn error(&mut self, err: io::Error, _ctx: &mut actix::Context<Self>) -> Running {
        error!(self.logger, "Stream error, closing cli server{:?}", err);
        Running::Stop
    }
}

#[derive(Clone)]
pub struct CliServerRef(Addr<CliServer>);

impl CliServerRef {
    pub fn subscribe(&self, sub: Recipient<CliMessageRcvd>) {
        self.0.do_send(Subscribe(sub))
    }
    pub fn stop(&self) {
        self.0.do_send(Stop);
    }
}

///////////////////////// Session //////////////////////////////////

pub type Ssid = usize;

struct Session {
    id: Ssid,
    framed: actix::io::FramedWrite<WriteHalf<UnixStream>, JsonCodec<ServerMessage>>,
    server: Addr<CliServer>,
    cli_msg_sub: Recipient<CliMessageRcvd>,
    logger: slog::Logger,
}

impl Session {
    pub fn stop(&mut self) {
        self.framed.close();
        self.server.do_send(UnregisterSession { id: self.id });
    }
}

impl Actor for Session {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // Register self in server.
        self.server
            .send(RegisterSession {
                id: self.id,
                addr: ctx.address(),
            }).into_actor(self)
            .then(|res, sess, ctx| {
                match res {
                    Err(err) => {
                        error!(sess.logger, "Session registration error {:?}", err);
                        ctx.stop()
                    }
                    _ => {}
                }
                actix::fut::ok(())
            }).wait(ctx); // wait for registration in server before processing any other events
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        trace!(self.logger, "Cli session stopped")
    }
}

impl actix::io::WriteHandler<io::Error> for Session {}

impl Handler<SendOutcome> for Session {
    type Result = ();

    fn handle(&mut self, msg: SendOutcome, _ctx: &mut Context<Session>){
        use serde_json;
        debug!(self.logger, "Sending outcome to cli client"; o!("outcome"=> serde_json::to_string_pretty(&msg.0).expect("Outcome should be always serializable")));
        self.framed.write(ServerMessage::Outcome(msg.0))
    }
}

impl Handler<SendErr> for Session {
    type Result = ();

    fn handle(&mut self, msg: SendErr, _ctx: &mut Context<Session>) {
        debug!(self.logger, "Sending error to cli client"; o!("Error"=> format!("{:?}",msg.0)));
        use kg_diag::Detail;
        self.framed.write(ServerMessage::Error(format!("{}", msg.0.as_fmt_display())))
    }
}

impl Handler<SendReachableInstances> for Session {
    type Result = ();

    fn handle(&mut self, msg: SendReachableInstances, _ctx: &mut Context<Session>) {
        debug!(self.logger, "Sending reachable instances to cli client"; o!("Instances"=> format!("{:?}",msg.0)));
        self.framed.write(ServerMessage::ReachableInstances(msg.0))
    }
}



impl Handler<Stop> for Session {
    type Result = ();

    fn handle(&mut self, _: Stop, _: &mut Self::Context) {
        self.stop();
    }
}

impl StreamHandler<CliMessage, io::Error> for Session {
    fn handle(&mut self, msg: CliMessage, ctx: &mut Self::Context) {
        debug!(self.logger, "Received message,"; o!("message"=>format!("{:?}", msg)));

        let sess = CliSessionRef(ctx.address());
        let msg = CliMessageRcvd::new(msg, sess);
        if self.cli_msg_sub.do_send(msg).is_err() {
            error!(self.logger, "cli_msg_sub dropped!")
        }
    }
    fn error(&mut self, err: io::Error, _ctx: &mut Self::Context) -> Running {
        error!(self.logger, "Stream error, closing session. {:?}", err);
        Running::Stop
    }

    fn finished(&mut self, _ctx: &mut Self::Context) {
        self.stop()
    }
}

#[derive(Clone)]
pub struct CliSessionRef(Addr<Session>);

impl CliSessionRef {
    pub fn send_outcome(&self, outcome: Outcome) {
        self.0.do_send(SendOutcome(outcome))
    }
    pub fn send_error(&self, err: RuntimeError) {
        self.0.do_send(SendErr(err))
    }
    pub fn send_reachable_instances(&self, rl: Vec<InstanceInfo>) {
        self.0.do_send(SendReachableInstances(rl))
    }
}
