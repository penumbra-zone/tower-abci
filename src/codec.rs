use std::marker::PhantomData;

use tokio_util::codec::{Decoder, Encoder};

use bytes::{BufMut, BytesMut};

pub struct Decode<M> {
    state: DecodeState,
    _marker: PhantomData<M>,
}

impl<M> Default for Decode<M> {
    fn default() -> Self {
        Self {
            state: DecodeState::Head,
            _marker: PhantomData,
        }
    }
}

#[derive(Debug)]
enum DecodeState {
    Head,
    Body { len: usize },
}

impl<M: prost::Message + Default> Decoder for Decode<M> {
    type Item = M;
    type Error = crate::BoxError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self.state {
            DecodeState::Head => {
                tracing::trace!(?src, "decoding head");

                // we don't use decode_varint directly, because it advances the
                // buffer regardless of success, but Decoder assumes that when
                // the buffer advances we've consumed the data. this is sort of
                // a sad hack, but it works.
                // fix this
                let mut tmp = src.clone().freeze();
                let len = match prost::encoding::decode_varint(&mut tmp) {
                    Ok(_) => {
                        // advance the real buffer
                        prost::encoding::decode_varint(src).unwrap() as usize
                    }
                    Err(_) => {
                        tracing::trace!(?self.state, src.len = src.len(), "waiting for header data");
                        return Ok(None);
                    }
                };
                self.state = DecodeState::Body { len };
                tracing::trace!(?self.state, "ready for body");

                // Recurse to attempt body decoding.
                self.decode(src)
            }
            DecodeState::Body { len } => {
                if src.len() < len {
                    tracing::trace!(?self.state, src.len = src.len(), "waiting for body");
                    return Ok(None);
                }

                let body = src.split_to(len);
                tracing::trace!(?body, "decoding body");
                let message = M::decode(body)?;

                // Now reset the decoder state for the next message.
                self.state = DecodeState::Head;

                Ok(Some(message))
            }
        }
    }
}

pub struct Encode<M> {
    _marker: PhantomData<M>,
}

impl<M> Default for Encode<M> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<M: prost::Message + Sized + std::fmt::Debug> Encoder<M> for Encode<M> {
    type Error = crate::BoxError;

    fn encode(&mut self, item: M, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // rewrite this to avoid extra copy?
        let mut buf = BytesMut::new();
        item.encode(&mut buf)?;
        let buf = buf.freeze();
        prost::encoding::encode_varint(buf.len() as u64, dst);
        dst.put(buf);

        Ok(())
    }
}
