# Build: docker build -t shelf .
# Run:   docker run -p 8080:8080 -v /path/to/books:/books shelf

FROM docker.io/library/node:22.22.2-trixie-slim AS frontend
COPY --from=docker.io/bufbuild/buf:1.75.0 /usr/local/bin/buf /usr/local/bin/buf
RUN npm install -g pnpm@12.15.0
WORKDIR /src
COPY frontend/package.json frontend/pnpm-lock.yaml frontend/
RUN pnpm --dir frontend install --frozen-lockfile
COPY buf.yaml buf.gen.yaml ./
COPY proto proto
COPY frontend frontend
RUN pnpm --dir frontend build

FROM docker.io/library/rust:1.99.1-slim-trixie AS server
WORKDIR /src
# `app` is only copied because cargo needs every workspace member to resolve the workspace.
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY app app
COPY proto proto
COPY --from=frontend /src/frontend/dist frontend/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p shelf-server \
    && cp target/release/shelf-server /usr/local/bin/

FROM docker.io/library/debian:trixie-slim
COPY --from=server /usr/local/bin/shelf-server /usr/local/bin/shelf-server
RUN mkdir /books && chown 1000:1000 /books
USER 1000:1000
VOLUME /books
EXPOSE 8080
CMD ["shelf-server"]
