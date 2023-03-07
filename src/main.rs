use std::error::Error;
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::task::Poll;

use actix_web::{self, FromRequest, HttpMessage, HttpRequest, HttpResponse, web};
use actix_web::dev::Payload;
use anyhow::{anyhow, Context};
use futures_core::{ready, Stream};
use photon_rs::transform::SamplingFilter;

use crate::operator::Op;

include!(concat!(env!("OUT_DIR"), "/image_command.rs"));

trait ImageProcess {
    fn process(&self, img: &photon_rs::PhotonImage) -> anyhow::Result<photon_rs::PhotonImage>;
}

impl ImageProcess for Resize {
    fn process(&self, img: &photon_rs::PhotonImage) -> anyhow::Result<photon_rs::PhotonImage> {
        Ok(photon_rs::transform::resize(
            img,
            self.w,
            self.h,
            SamplingFilter::Nearest,
        ))
    }
}

impl ImageProcess for Rotate {
    fn process(&self, img: &photon_rs::PhotonImage) -> anyhow::Result<photon_rs::PhotonImage> {
        Ok(photon_rs::transform::rotate(img, self.angle))
    }
}

macro_rules! impl_image_proc {
    ($typ:ty , $($name:ident),*) => {
        impl From<$typ> for Box<dyn ImageProcess> {
            fn from(value: $typ) -> Self {
                use $typ as Type;
                match value {
                    $(
                    Type::$name(v) => {
                        Box::new(v)
                    }
                    )*
                }
            }
        }
    };
}

impl_image_proc!(Op, Resize, Rotate);

type HttpResult = anyhow::Result<HttpResponse, Box<dyn Error>>;

async fn img_proc_request(req_body: Proto<ImageCommand>) -> HttpResult {
    let req = req_body.deref();
    let image_bytes = reqwest::get(&req.image_url).await?.bytes().await?;
    let mut img = photon_rs::PhotonImage::new_from_byteslice(image_bytes.to_vec());
    let ops = req
        .ops
        .clone()
        .into_iter()
        .filter_map(|s| s.op)
        .map(|v| v.into())
        .collect::<Vec<Box<dyn ImageProcess>>>();
    for op in ops {
        img = op.process(&img)?;
    }
    let out_jpg_bytes: bytes::Bytes = img.get_bytes_jpeg(100).into();
    Ok(HttpResponse::Ok()
        .content_type(mime::IMAGE_JPEG)
        .body(out_jpg_bytes))
}

#[derive(Debug)]
struct DecodeProtoError(anyhow::Error);

impl<E: std::error::Error + Send + Sync + 'static> From<E> for DecodeProtoError {
    fn from(value: E) -> Self {
        DecodeProtoError(anyhow::Error::new(value))
    }
}

impl Display for DecodeProtoError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl actix_web::ResponseError for DecodeProtoError {}

struct Proto<T>(T);

impl<T> Deref for Proto<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Proto<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

enum DecodeProto<T> {
    Parse {
        payload: Payload,
        buf: bytes::BytesMut,
        _phantom: PhantomData<T>,
    },
    JustError(Option<anyhow::Error>),
}

impl<T> Unpin for DecodeProto<T> {}

impl<T> std::future::Future for DecodeProto<T>
    where
        T: prost::Message + Default,
{
    type Output = std::result::Result<Proto<T>, DecodeProtoError>;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let me = self.get_mut();
        match me {
            DecodeProto::Parse { payload, buf, .. } => loop {
                let res = ready!(Pin::new(&mut *payload).poll_next(cx));
                match res {
                    Some(chunk) => {
                        let chunk = chunk?;
                        buf.extend_from_slice(&chunk);
                    }
                    None => {
                        // read finished
                        println!("msg:{}", String::from_utf8_lossy(buf));
                        let v = T::decode(buf)?;
                        return Poll::Ready(Ok(Proto(v)));
                    }
                }
            },
            DecodeProto::JustError(s) =>
                Poll::Ready(Err(DecodeProtoError(s.take().unwrap()))),
        }
    }
}

impl<T> FromRequest for Proto<T>
    where
        T: prost::Message + Default,
{
    type Error = DecodeProtoError;
    type Future = DecodeProto<T>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let content_type_ok = req
            .mime_type()
            .unwrap_or_default()
            .map(|v| mime::APPLICATION_OCTET_STREAM == v)
            .unwrap_or(false);
        if !content_type_ok {
            return DecodeProto::JustError(anyhow!("content type mismatched!").into());
        }
        DecodeProto::Parse {
            payload: payload.take(),
            buf: Default::default(),
            _phantom: PhantomData,
        }
    }
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let server = actix_web::HttpServer::new(move || {
        actix_web::App::new()
            .service(
                web::resource("/image_proc")
                    .route(web::post().to(img_proc_request)))
    })
        .bind("127.0.0.1:12345")?
        .run();
    server.await.context("server exit")
}
