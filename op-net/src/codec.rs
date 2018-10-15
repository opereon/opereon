use tokio_codec::{Decoder, Encoder};

use bytes::BytesMut;

use std::io;

//use rmps::{Deserializer, Serializer};

use serde_json;
use serde::{Serialize};
use serde::export::PhantomData;
use serde::de::DeserializeOwned;

pub struct JsonCodec<T: Serialize + DeserializeOwned>(PhantomData<T>);

impl<T: Serialize + DeserializeOwned> JsonCodec<T> {
    pub fn new() -> JsonCodec<T> {
        JsonCodec(PhantomData)
    }
}

impl<T: Serialize + DeserializeOwned> Decoder for JsonCodec<T> {
    type Item = T;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {

        // TODO use message pack instead of json. https://github.com/3Hren/msgpack-rust/issues/170

        if src.len() == 0 {
            return Ok(None);
        }

        let (result, offset) = {
            let de = serde_json::Deserializer::from_reader(src.as_ref());

            let mut stream = de.into_iter::<Self::Item>();

            let result = if let Some(res) = stream.next() {
                match res {
                    Ok(val) => {
                        Ok(Some(val))
                    }
                    Err(err) => {
                        eprintln!("Deserialization error{:?}", err);
                        Err(io::Error::new(io::ErrorKind::Other, "Deserizalization error"))
                    }
                }
            } else {
                Ok(None)
            };

            (result, stream.byte_offset())
        };

        src.split_to(offset);

        result

//        let msg: Message;
//        {
//            msg = match serde_json::from_reader(src.as_ref()) {
//                Ok(msg) => msg,
//                Err(err) => {
//                    eprintln!("Deserialization error{:?}", err);
//                    return Err(io::Error::new(io::ErrorKind::Other, "Deserizalization error"));
//                }
//            };
//        }
//        src.clear();
//        Ok(Some(msg))
    }
}

impl<T: Serialize +DeserializeOwned> Encoder for JsonCodec<T> {
    type Item = T;
    type Error = io::Error;

    fn encode(
        &mut self, msg: Self::Item, dst: &mut BytesMut,
    ) -> Result<(), Self::Error> {
//        let msg = json::to_string(&msg).unwrap();
//        let msg_ref: &[u8] = msg.as_ref();
//
//        dst.reserve(msg_ref.len() + 2);
//        dst.put_u16_be(msg_ref.len() as u16);
//        dst.put(msg_ref);
//
//        Ok(())
        let ser = serde_json::to_vec(&msg).expect("Serialization error");

        dst.extend_from_slice(ser.as_ref());
        Ok(())
    }
}
