use actix::prelude::*;

use std::io;
use std::path::Path;

use tokio::io::AsyncRead;
use tokio::io::WriteHalf;
use tokio_codec::FramedRead;
use tokio_uds::UnixStream;

use futures;
use futures::future::Future;

use super::display;
use cli_client::display::DisplayFormat;
use commons::*;
use op_net::JsonCodec;

pub type ClientFramed = actix::io::FramedWrite<WriteHalf<UnixStream>, JsonCodec<CliMessage>>;

pub struct Client {
    framed: ClientFramed,
    format: DisplayFormat,
}

impl Client {
    pub fn new(framed: ClientFramed, format: DisplayFormat) -> Client {
        Client { framed, format }
    }

    pub fn send(&mut self, msg: CliMessage) {
        self.framed.write(msg)
    }

    pub fn connect_and_send(sock_path: &Path, msg: CliMessage, format: DisplayFormat) {
        let sock_path = sock_path.to_path_buf();
        let client_fut = UnixStream::connect(&sock_path)
            .map_err(move |err| {
                println!("Cannot connect to Operon socket : {:?}, {}", sock_path, err);
                System::current().stop();
            }).and_then(move |stream| {
                let (r, w) = stream.split();
                Client::create(move |ctx| {
                    ctx.add_stream(FramedRead::new(r, JsonCodec::new()));

                    let framed = actix::io::FramedWrite::new(w, JsonCodec::new(), ctx);

                    let mut client = Client::new(framed, format);

                    client.send(msg);
                    client
                });
                futures::future::ok(())
            });

        Arbiter::spawn(client_fut);
    }
}

impl Actor for Client {
    type Context = actix::Context<Self>;

    //    fn started(&mut self, ctx: &mut actix::Context<Client>) {
    //        println!("Client actor started");
    //    }

    fn stopped(&mut self, _ctx: &mut <Self as Actor>::Context) {
        System::current().stop();
    }
}

impl actix::io::WriteHandler<io::Error> for Client {}

impl StreamHandler<ServerMessage, io::Error> for Client {
    fn handle(&mut self, msg: ServerMessage, ctx: &mut actix::Context<Self>) {
        //        eprintln!("Message from server received = {:#?}", msg);
        match msg {
            ServerMessage::Outcome(outcome) => display::display_outcome(&outcome, self.format),
            ServerMessage::Error(err) => {
                eprintln!("Operon error = {:?}", err);
            }
            ServerMessage::ReachableInstances(rl) => {
                eprintln!("Reachable instances {} : {:#?}", rl.len(), rl);
            }
            ServerMessage::Progress => unimplemented!(),
        }
        ctx.stop();
    }

    fn error(&mut self, err: io::Error, _ctx: &mut actix::Context<Self>) -> Running {
        eprintln!("Connection error, closing {:?}", err);
        Running::Stop
    }
}
