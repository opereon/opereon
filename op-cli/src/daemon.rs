use actix::actors::signal;
use actix::prelude::*;

use super::*;

use op_net;
use op_net::MessageReceived;
use op_net::MessageResponse;
use op_net::ServerRef as NetServerRef;

use op_exec::{ConfigRef, EngineRef, OperationRef};

use cli_server::CliServer;
use cli_server::CliServerRef;

use actix::actors::signal::ProcessSignals;
use actix::actors::signal::Subscribe;

use slog;

use futures::Future;
use op_net::NodeReachable;
use op_net::NodeUnreachable;
use std::io;
use std::path::Path;
use uuid::Uuid;

use cli_server::CliMessageRcvd;
use commons::CliMessage;
use core::fmt;
use serde::Deserialize;
use serde::Serialize;
use serde_json;

type InstanceId = Uuid;

pub trait ToNetMessage {
    fn to_net_msg(&self) -> op_net::Message;
}

impl<T> ToNetMessage for T
where
    T: Serialize,
{
    fn to_net_msg(&self) -> op_net::Message {
        serde_json::to_vec(self).unwrap()
    }
}

fn deserialize_net_msg<'a, T>(msg: &'a op_net::Message) -> Result<T, serde_json::Error>
where
    T: Deserialize<'a>,
{
    serde_json::from_slice::<T>(msg.as_ref())
}

#[derive(Debug, Serialize, Deserialize)]
enum OpMessage {
    InstanceInfoReq,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    id: InstanceId,
    // some important fields
}

impl InstanceInfo {
    fn new(id: InstanceId) -> InstanceInfo {
        InstanceInfo { id }
    }
}

impl fmt::Debug for InstanceInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("InstanceInfo")
            .field("id", &self.id.to_string())
            .finish()
    }
}

pub struct Daemon {
    engine: EngineRef,
    cli: CliServerRef,
    net: NetServerRef,
    reachable_instances: LinkedHashMap<InstanceId, InstanceInfo>,
    this_instance: InstanceInfo,
    logger: slog::Logger,
}

impl Daemon {
    pub fn run(config: ConfigRef, logger: slog::Logger) {
        let engine = check(EngineRef::start(config.clone(), logger.clone()));

        let cli = start_cli_server(config.daemon().socket_path(), logger.clone());

        let net_config = {
            let e = engine.read();
            e.config().net().clone()
        };

        let net = start_h2h_server(net_config.clone());

        Arbiter::spawn(engine.clone());

        let daemon = Daemon {
            engine,
            cli: cli.clone(),
            net: net.clone(),
            reachable_instances: LinkedHashMap::new(),
            this_instance: InstanceInfo::new(net_config.this_node().id().clone()),
            logger,
        }.start();

        let sigs = System::current().registry().get::<ProcessSignals>();

        sigs.do_send(Subscribe(daemon.clone().recipient()));

        cli.subscribe(daemon.clone().recipient());

        net.subscribe_msg_recvd(daemon.clone().recipient());
        net.subscribe_node_reachable(daemon.clone().recipient());
        net.subscribe_node_unreachable(daemon.recipient());
    }

    fn add_reachable_instance(&mut self, instance: InstanceInfo) {
        self.reachable_instances.insert(instance.id, instance);
    }

    fn remove_reachable_instance(&mut self, instance_id: &InstanceId) {
        self.reachable_instances.remove(instance_id);
    }

    fn request_instance_info(
        &self,
        instance_id: &InstanceId,
    ) -> impl Future<Item = InstanceInfo, Error = io::Error> {
        self.net
            .request(instance_id, OpMessage::InstanceInfoReq.to_net_msg())
            .and_then(|resp| match deserialize_net_msg::<InstanceInfo>(&resp) {
                Ok(msg) => Ok(msg),
                Err(_err) => Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Cannot Deserialize InstanceInfo",
                )),
            })
    }

    fn this_instance(&self) -> &InstanceInfo {
        &self.this_instance
    }

    fn stop(&mut self, _ctx: &mut Context<Daemon>) {
        self.net.stop();
        self.cli.stop();
        self.engine.stop();

        System::current().stop();
    }
}

impl Actor for Daemon {
    type Context = Context<Daemon>;

    fn started(&mut self, ctx: &mut Context<Daemon>) {
        let fut = self
            .net
            .get_reachable_nodes()
            .into_actor(self)
            .map_err(|err, daemon, _ctx| {
                warn!(daemon.logger, "Cannot get reachable instances: {:?}", err);
            }).map(
                |reachable_instances: Vec<InstanceId>,
                 daemon: &mut Daemon,
                 ctx: &mut Context<Daemon>| {
                    for instance_id in reachable_instances {
                        let req_fut = daemon
                            .request_instance_info(&instance_id)
                            .into_actor(daemon)
                            .map(|info, daemon, _ctx| {
                                daemon.add_reachable_instance(info);
                            }).map_err(move |err, daemon, _ctx| {
                                daemon.remove_reachable_instance(&instance_id);
                                warn!(daemon.logger, "Instance info request failed: {:?}", err);
                            });
                        ctx.spawn(req_fut);
                    }
                },
            );
        ctx.spawn(fut);

        //        ctx.run_later(Duration::from_secs(4), |daemon, ctx| {
        //            let msg = b"Hello!".to_vec();
        //
        //            let fut
        //            = daemon.net.broadcast(msg)
        //                .map(|_| {
        //                    println!("Broadcast sent");
        //                })
        //                .map_err(|err| {
        //                    eprintln!("Error sending broadcast = {:?}", err);
        //                });
        //
        //            ctx.spawn(fut.into_actor(daemon));
        //        });
    }

    fn stopped(&mut self, _ctx: &mut Context<Daemon>) {
        System::current().stop();
    }
}

// handle network messages
impl Handler<MessageReceived> for Daemon {
    type Result = MessageResponse;

    fn handle(&mut self, msg: MessageReceived, _ctx: &mut Self::Context) -> Self::Result {
        let _sender = msg.sender;

        let msg = match deserialize_net_msg(&msg.msg) {
            Ok(msg) => msg,
            Err(err) => {
                warn!(self.logger, "Malformed message received: {:?}", err);
                return MessageResponse::None;
            }
        };

        match msg {
            OpMessage::InstanceInfoReq => {
                return MessageResponse::Sync(self.this_instance().to_net_msg())
            }
        }

        //        eprintln!("Message received = {}, {:?}", &msg.sender, String::from_utf8(msg.msg.clone()).unwrap());
        //
        //        if msg.msg == b"Example request".to_vec() {
        //            let resp = b"Example response".to_vec();
        ////            return MessageResponse::Sync(resp);
        //            return MessageResponse::new_async(futures::future::ok(Some(resp)));
        //        }
        //
        //        if msg.msg != b"Hello!".to_vec() {
        //            return MessageResponse::None;
        //        }
        //
        //        let req = b"Example request".to_vec();
        //        let sender = msg.sender;
        //
        //        let fut
        //        = self.net.request(&sender, req)
        //            .map(move |resp| {
        //                println!("Response from {} received: {}", sender, String::from_utf8(resp).unwrap());
        //            })
        //            .map_err(|err| {
        //                eprintln!("Request error = {:?}", err);
        //            });
        //
        //        Arbiter::spawn(fut);
    }
}

impl Handler<NodeUnreachable> for Daemon {
    type Result = ();

    fn handle(&mut self, msg: NodeUnreachable, _ctx: &mut Context<Daemon>) {
        self.remove_reachable_instance(&msg.node);
    }
}

impl Handler<NodeReachable> for Daemon {
    type Result = ();

    fn handle(&mut self, msg: NodeReachable, ctx: &mut Context<Daemon>) {
        let instance_id = msg.node;
        let fut = self
            .request_instance_info(&instance_id)
            .into_actor(self)
            .map(|info, daemon, _ctx| {
                daemon.add_reachable_instance(info);
            }).map_err(move |err, daemon, _ctx| {
                warn!(daemon.logger, "Instance info request failed: {:?}", err);
                daemon.remove_reachable_instance(&instance_id)
            });
        ctx.spawn(fut);
    }
}

// handle cli messages
impl Handler<CliMessageRcvd> for Daemon {
    type Result = ();

    fn handle(&mut self, msg: CliMessageRcvd, ctx: &mut Context<Daemon>) {
        let session = msg.session;
        let msg = msg.msg;
        match msg {
            CliMessage::Execute(op) => {
                let op: OperationRef = op.into();

                let out_fut = self
                    .engine
                    .enqueue_operation(op, false)
                    .unwrap() // FIXME handle errors
                    .then(move |res| {
                        // Send outcome to cli client
                        match res {
                            Ok(outcome) => session.send_outcome(outcome),
                            Err(err) => session.send_error(err),
                        }
                        Ok(())
                    });
                ctx.spawn(out_fut.into_actor(self));
            }
            CliMessage::GetReachableInstances => {
                let rl = self.reachable_instances.values().cloned().collect();
                session.send_reachable_instances(rl);
            }
            CliMessage::Cancel => unimplemented!(),
        }
    }
}

// Handle unix signals
impl Handler<signal::Signal> for Daemon {
    type Result = ();
    fn handle(&mut self, msg: signal::Signal, ctx: &mut Context<Self>) {
        match msg.0 {
            signal::SignalType::Int => {
                info!(self.logger, "SIGINT signal received, stopping daemon");
                self.stop(ctx)
            }
            signal::SignalType::Term => {
                info!(self.logger, "SIGTERM signal received, stopping daemon");
                self.stop(ctx)
            }
            signal::SignalType::Quit => {
                info!(self.logger, "SIGQUIT signal received, stopping daemon");
                self.stop(ctx)
            }
            _ => warn!(self.logger, "Unhandled signal received: {:?}", msg.0),
        }
    }
}

fn start_h2h_server(config: op_net::Config) -> NetServerRef {
    let h2h_addr = NetServerRef::start(config).expect("Cannot start network server");
    h2h_addr
}

fn start_cli_server(socket_path: &Path, logger: slog::Logger) -> CliServerRef {
    match CliServer::start(socket_path, logger.clone()) {
        Ok(addr) => addr,
        Err(err) => {
            error!(logger, "Cannot start cli server: {:?}", err);
            panic!("Cannot start cli server")
        }
    }
}
