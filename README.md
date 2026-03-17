# BetMasters

Generador distribuido de fractales de Mandelbrot en Rust, diseñado para múltiples máquinas conectadas vía VPN WireGuard.

---

## Tabla de Contenidos

1. [Descripción del Proyecto](#1-descripción-del-proyecto)
2. [Arquitectura de Red: VPN WireGuard](#2-arquitectura-de-red-vpn-wireguard)
3. [Arquitectura Distribuida](#3-arquitectura-distribuida)
4. [Estructura del Código](#4-estructura-del-código)
5. [Algoritmo de Mandelbrot](#5-algoritmo-de-mandelbrot)
6. [Decisiones de Diseño](#6-decisiones-de-diseño)
7. [Ejecución del Proyecto](#7-ejecución-del-proyecto)
8. [Docker](#8-docker)

---

## 1. Descripción del Proyecto

BetMasters divide una imagen Mandelbrot en bloques de filas horizontales, los distribuye a workers para su cómputo, y el coordinador reensambla los resultados en un PNG final.

Opera sobre una **VPN WireGuard**: todos los nodos comparten una red privada cifrada. Los workers solo necesitan conocer la IP VPN del coordinador — sin IPs públicas ni descubrimiento de servicios.

Un único binario maneja ambos roles, seleccionado en tiempo de ejecución via `APP_ROLE`.

### Inicio Rápido con Docker

```bash
cd docker
cp .env.example .env
docker compose up --build
```

Levanta 1 coordinador + 9 workers automáticamente. El PNG se guarda en `fractal.png` al completarse.

### Obtener el PNG

```bash
docker cp con:fractal.png ./fractal.png
```

---

## 2. Arquitectura de Red: VPN WireGuard

Topología **Hub-and-Spoke** (`10.10.10.0/24`):

```
            ┌─────────────────────────────┐
            │  Hub: Coordinador           │
            │  10.10.10.1 (Ubuntu server) │
            └──────────────┬──────────────┘
               túnel wg    │    túnel wg
          ┌────────────────┘────────────────┐
     ┌────┴────┐                      ┌─────┴─────┐
     │ Worker  │                      │  Worker   │
     │10.10.10.2│                     │10.10.10.3 │
     │ Laptop  │                      │ Laptop    │
     └─────────┘                      └───────────┘
```

- El coordinador corre en el Hub con IP VPN fija y puerto UDP `51820` abierto.
- Los workers son Spokes; alcanzan al coordinador por su IP VPN desde cualquier red.
- Todo el tráfico está cifrado por WireGuard. Los workers no necesitan IPs públicas.

---

## 3. Arquitectura Distribuida

| Rol | Responsabilidad |
|-----|-----------------|
| **Coordinador** | Divide la imagen en tareas, las distribuye, rastrea completitud, detecta timeouts, guarda el PNG. |
| **Worker** | Se conecta vía WebSocket, recibe bloques de filas, computa Mandelbrot, devuelve resultados. |

### Flujo de Comunicación (WebSocket en `/ws`)

```
Worker                          Coordinador
  │──── WS Connect ───────────>│
  │──── Hello ─────────────────>│  (alias asignado)
  │<─── Compute {task_id, rows}─│
  │  [computa Mandelbrot]        │
  │──── ComputeResult ──────────>│  (datos fusionados)
  │<─── Compute ...             │  (siguiente tarea)
  │                        [todas listas → fractal.png]
```

### Internos del Coordinador

Bucle `tokio::select!` con dos fuentes de eventos:
- **Tick 1s**: verifica timeouts (3s por tarea, re-encola si vence); activa auto-inicio tras 10s sin workers.
- **Canal `mpsc`**: recibe `WorkerConnected`, `WorkerDisconnected`, `WorkerFinished` desde los handlers de Axum.

Todo el estado mutable vive en una única tarea async — sin `Arc<Mutex<>>`.

### Internos del Worker

Bucle infinito de reconexión (retry cada 2s). Por sesión: envía `Hello` → espera `Compute` → llama `compute_block()` → envía `ComputeResult`.

---

## 4. Estructura del Código

```
betmasters/
├── .github/workflows/gatekeeper.yml   # PRs a main solo desde development
├── docker/
│   ├── .env.example
│   └── docker-compose.yml
├── docs/
├── vpn/
└── rust/src/
    ├── main.rs                # Lee APP_ROLE, construye Coordinator o Worker
    ├── logger.rs
    ├── coordinator/
    │   ├── logic.rs           # Estado, cola de tareas, bucle select!, guardado PNG
    │   ├── handlers.rs        # Handlers HTTP + WebSocket (Axum)
    │   └── types.rs           # CoordinatorCommand, WorkerMessage, InternalMessage, Task
    └── worker/
        ├── logic.rs           # Worker::run(), sesión WS, reconexión
        ├── math.rs            # compute_block(), evaluate() — cómputo Mandelbrot
        └── types.rs           # Struct Worker
```

---

## 5. Algoritmo de Mandelbrot

Para cada punto `c = x + yi`, se itera `z_{n+1} = z_n² + c` (con `z₀ = 0`). El punto escapa si `|z|² > 4`; se considera interior si llega a `MAX_ITERS` sin escapar (renderizado en negro).

**Coordenadas de píxel a plano complejo:**
```
c_x = x_min + col × (x_max - x_min) / ancho
c_y = y_min + fila × (y_max - y_min) / alto
```

**Iteración (algebraica, sin números complejos):**
```
Re(z² + c) = a² - b² + c_x
Im(z² + c) = 2ab   + c_y
```

**Colorización (polinomios de Bernstein):** con `t = iter / MAX_ITERS`:
```
R = 9.0  × (1-t) × t³  × 255
G = 15.0 × (1-t)² × t² × 255
B = 8.5  × (1-t)³ × t  × 255
```
Produce un gradiente continuo de azul/verde oscuro (escape lento) a naranja/amarillo (escape rápido).

---

## 6. Decisiones de Diseño

| Decisión | Por qué | Trade-off |
|----------|---------|-----------|
| **Binario único + `APP_ROLE`** | Mismo artefacto en todos los nodos; rol por env var, sin recompilación. | Árbol de dependencias compartido, binario ligeramente mayor. |
| **WebSocket persistente** | Bidireccional; el coordinador envía tareas sin polling. Encaja con Hub-and-Spoke. | Una conexión abierta por worker (insignificante a esta escala). |
| **`mpsc` en vez de `Arc<Mutex<>>`** | Estado mutable en una sola tarea async; sin contención de locks ni riesgo de deadlocks. | El bucle del coordinador es cuello de botella para mutaciones (aceptable). |
| **`spawn_blocking` para el PNG** | Codificar 4K con compresión bloquearía el event loop de Tokio. | Ninguno relevante. |

---

## 7. Ejecución del Proyecto

Todos los comandos desde `rust/`.

```bash
# Build
cargo build && cargo build --release

# Coordinador
APP_ROLE=coordinator PORT=8080 IMAGE_WIDTH=3840 IMAGE_HEIGHT=2160 \
BLOCK_SIZE=100 MAX_ITERS=1000 cargo run

# Worker
APP_ROLE=worker PORT=8081 COORDINATOR_URL=http://10.10.10.1:8080 \
MAX_ITERS=1000 X_MIN=-2.0 X_MAX=1.0 Y_MIN=-1.5 Y_MAX=1.5 cargo run
```

### Variables de Entorno

| Variable | Rol | Default | Descripción |
|----------|-----|---------|-------------|
| `APP_ROLE` | ambos | `coordinator` | `coordinator` o `worker` |
| `PORT` | ambos | `8080` | Puerto de escucha |
| `COORDINATOR_URL` | worker | `http://127.0.0.1:8080` | URL del coordinador |
| `IMAGE_WIDTH` / `IMAGE_HEIGHT` | coordinador | `3840` / `2160` | Dimensiones de salida |
| `BLOCK_SIZE` | coordinador | `100` | Filas por tarea |
| `MAX_ITERS` | ambos | `1000` | Límite de iteraciones |
| `X_MIN` / `X_MAX` / `Y_MIN` / `Y_MAX` | worker | `-2.0` / `1.0` / `-1.5` / `1.5` | Límites del plano complejo |
| `OUTPUT_FILE` | coordinador | `fractal.png` | Ruta del PNG de salida |
| `BIND_ADDR` | coordinador | `0.0.0.0` | Dirección de bind |

---

## 8. Docker

```bash
cd docker && cp .env.example .env
# Editar .env según sea necesario
docker compose up --build
```

---

## Flujo de Git

PRs a `main` deben originarse desde `development`, aplicado por `.github/workflows/gatekeeper.yml`.
