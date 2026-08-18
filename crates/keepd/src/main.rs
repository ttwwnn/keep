use anyhow::Result;

fn main() -> Result<()> {
    let path = keep_proto::socket_path();
    let server = keepd::Server::bind(&path)?;
    eprintln!("keepd listening on {}", server.path().display());
    server.run()
}
