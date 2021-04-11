use std::marker::PhantomData;

use tokio_util::codec::{Decoder, Encoder};

use bytes::BytesMut;

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
                // Check if we have enough data to decode any LEB128-encoded u64
                // This assumes message bodies are longer than 9 bytes
                // XXX: probably want to cut this to 5 (u32 size) since we
                // almost certainly don't want to read bigger than 4GB messages?
                if src.len() < 10 {
                    tracing::trace!(?self.state, src.len = src.len(), "waiting for header data");
                    return Ok(None);
                }
                // XXX: decoding length checks should go here
                let len = prost::encoding::decode_varint(src)? as usize;
                self.state = DecodeState::Body { len };

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
        let len = item.encoded_len() + 10;
        if dst.capacity() < len {
            dst.reserve(len);
        }

        item.encode_length_delimited(dst)?;
        Ok(())
    }
}
