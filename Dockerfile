FROM rust:alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked --bin mcp-multiplexer

FROM alpine:3
RUN apk add --no-cache ca-certificates
COPY --from=build /app/target/release/mcp-multiplexer /usr/local/bin/
ENTRYPOINT ["mcp-multiplexer", "--config", "/config/.mcp.json"]
