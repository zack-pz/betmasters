# Etapa de compilación
FROM rust:alpine AS builder

# Instalar dependencias de compilación para musl
RUN apk add --no-cache musl-dev ca-certificates

WORKDIR /usr/src/app

# Copiar archivos de dependencias
COPY Cargo.toml Cargo.lock ./

# Crear un proyecto dummy para cachear la compilación de librerías
RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    cargo build --release --target x86_64-unknown-linux-musl && \
    rm -rf src

# Copiar el código fuente y compilar el binario real
COPY src ./src
RUN touch src/main.rs && \
    cargo build --release --target x86_64-unknown-linux-musl

# Etapa de ejecución
FROM alpine:latest

# Instalar herramientas de red y WireGuard
RUN apk add --no-cache wireguard-tools iptables ca-certificates

# Copiar el binario estático desde la etapa de compilación
COPY --from=builder /usr/src/app/target/x86_64-unknown-linux-musl/release/core /core

# Ejecutar el binario directamente
ENTRYPOINT ["/core"]
