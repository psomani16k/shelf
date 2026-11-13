use std::path::PathBuf;

use shelf_server::Config;

command! {
    /// Runs the real server as the Docker image does, with `data` in place of `/data` and
    /// `books` in place of `/books`, on 0.0.0.0:8080. Ctrl-C stops it.
    pub async fn server(&self, data: PathBuf, books: PathBuf) -> anyhow::Result<()> {
        shelf_server::serve(Config {
            data,
            books,
            ..Default::default()
        })
        .await
    }
}
