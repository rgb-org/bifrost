use core::borrow::BorrowMut;
use std::convert::Into;
use std::io::Read;
use std::marker::{Send, Sync};
use std::path::Path;

use bitcoin::OutPoint;
use bitcoin::util::hash::Sha256dHash;
use hyper::method::Method;
use hyper::method::Method::{Get, Post};
use hyper::net::Fresh;
use hyper::NotFound;
use hyper::server::{Request, Response, Server};
use hyper::server::Handler;
use hyper::uri::RequestUri::AbsolutePath;
use rgb::proof::Proof;

pub trait BifrostDatabase {
    fn new(basedir: &Path) -> Self;
    fn get_proofs_for(&self, outpoint: &OutPoint) -> Vec<Proof>;
    fn save_proof(&self, proof: &Proof, txid: &Sha256dHash);
}

struct RGBServer<D: BifrostDatabase + Sync + Send> {
    database: Box<D>
}

impl<D> RGBServer<D>
    where D: BifrostDatabase + Sync + Send
{
    pub fn new(database: D) -> RGBServer<D> {
        RGBServer {
            database: Box::new(database)
        }
    }

    fn get_outpoint(&self, path: &String) -> Option<OutPoint> {
        let parts: Vec<&str> = path.split("/").collect();

        if parts.len() != 2 {
            eprintln!("Invalid request");
            return None;
        }

        let parts: Vec<&str> = parts[1].split(":").collect();

        Some(OutPoint {
            txid: Sha256dHash::from_hex(parts[0]).unwrap(),
            vout: parts[1].parse().unwrap()
        })
    }
}

impl<D> Handler for RGBServer<D>
    where D: BifrostDatabase + Sync + Send
{
    fn handle<'a, 'k>(&'a self, mut req: Request<'a, 'k>, mut res: Response<'a, Fresh>) {
        let mut buffer: Vec<u8> = Vec::new();
        req.borrow_mut().read_to_end(&mut buffer);

        match req.uri {
            AbsolutePath(ref path) => {
                if req.method == Get {
                    let outpoint = self.get_outpoint(path).unwrap();
                    let proofs = self.database.get_proofs_for(&outpoint);

                    use bitcoin::network::serialize::RawEncoder;
                    use bitcoin::network::encodable::ConsensusEncodable;

                    let mut encoded: Vec<u8> = Vec::new();
                    let mut enc = RawEncoder::new(encoded);
                    proofs.consensus_encode(&mut enc);

                    res.send(&enc.into_inner()).unwrap();

                    println!("Downloaded {} proofs for {}", proofs.len(), outpoint);

                    return;
                } else if req.method == Post {
                    let outpoint = self.get_outpoint(path).unwrap();

                    use bitcoin::network::serialize::deserialize;
                    let decoded: Proof = deserialize(&mut buffer).unwrap();

                    println!("Uploaded proof for {}", outpoint);

                    self.database.save_proof(&decoded, &outpoint.txid);
                } else {
                    *res.status_mut() = NotFound;
                    return;
                }
            },
            _ => {
                return;
            }
        };
    }
}

pub fn start_server<D: 'static + BifrostDatabase>(port: String, database: D)
    where D: BifrostDatabase + Sync + Send
{
    let rgb_server = RGBServer::new(database);

    let _listening = Server::http(format!("0.0.0.0:{}", port)).unwrap()
        .handle(rgb_server);
    println!("Listening on http://0.0.0.0:{}", port);
}