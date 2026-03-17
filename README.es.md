# BetMasters

Un generador distribuido de fractales de Mandelbrot implementado en Rust, diseñado para ejecutarse en múltiples máquinas conectadas a través de una VPN WireGuard.

---

## Tabla de Contenidos

1. [Descripción del Proyecto](#1-descripción-del-proyecto)
2. [Arquitectura de Red: VPN WireGuard](#2-arquitectura-de-red-vpn-wireguard)
3. [Arquitectura Distribuida: Coordinador y Workers](#3-arquitectura-distribuida-coordinador-y-workers)
4. [Estructura del Código](#4-estructura-del-código)
5. [Algoritmo de Mandelbrot](#5-algoritmo-de-mandelbrot)
6. [Decisiones de Diseño](#6-decisiones-de-diseño)
7. [Ejecución del Proyecto](#7-ejecución-del-proyecto)
8. [Docker](#8-docker)

---

## 1. Descripción del Proyecto

BetMasters calcula imágenes del fractal de Mandelbrot distribuyendo la carga de trabajo entre múltiples máquinas. La imagen se divide en bloques de filas horizontales; cada bloque se envía a un proceso worker para su cómputo, y el coordinador reensambla los resultados en un archivo PNG final.

El sistema fue diseñado para operar sobre una **VPN WireGuard** — todos los nodos (coordinador y workers) comparten una red privada cifrada, y los workers pueden ejecutarse en cualquier dispositivo (laptop, servidor, VM en la nube) siempre que estén conectados a la VPN. Esto elimina la necesidad de IPs públicas o descubrimiento de servicios complejo: los workers solo necesitan conocer la IP VPN estable del coordinador.

Un único binario compilado maneja ambos roles. El rol se selecciona en tiempo de ejecución mediante la variable de entorno `APP_ROLE`, lo que simplifica el despliegue: el mismo binario se distribuye a cada nodo, y el rol se determina únicamente por configuración.

---

## 2. Arquitectura de Red: VPN WireGuard

El proyecto funciona con una topología WireGuard de **Hub-and-Spoke**:

```
                    ┌─────────────────────────────┐
                    │       VPN WireGuard          │
                    │         10.10.10.0/24          │
                    │                              │
        ┌───────────┤  Hub: Coordinador            │
        │           │  10.10.10.1 (Ubuntu server)  │
        │           └──────────────────────────────┘
        │                        │
   túnel wg                 túnel wg
        │                        │
   ┌────┴────┐             ┌──────┴────┐
   │ Worker  │             │  Worker   │
   │10.10.10.2 │           │ 10.10.10.3 │
   │ Laptop  │             │ AWS EC2   │
   └─────────┘             └───────────┘
```

- El **coordinador** corre en el nodo Hub, que tiene una IP VPN fija (p. ej. `10.10.10.1`) y un puerto UDP abierto para WireGuard (`51820`).
- Los **workers** son Spokes que se conectan a la VPN desde cualquier dispositivo. Una vez conectados, alcanzan al coordinador en su IP VPN — sin importar la ubicación física o la red.
- Todo el tráfico está cifrado de extremo a extremo por WireGuard. Los workers no necesitan IPs públicas ni puertos de entrada abiertos.

La VPN está configurada para que los Spokes puedan comunicarse con el Hub pero no directamente entre sí, lo cual es suficiente para esta arquitectura ya que toda la coordinación fluye a través del coordinador.

---

## 3. Arquitectura Distribuida: Coordinador y Workers

### Roles

| Rol | Responsabilidad |
|-----|-----------------|
| **Coordinador** | Divide la imagen en tareas, las distribuye a los workers, rastrea su completitud, detecta timeouts y guarda el PNG final. |
| **Worker** | Se conecta al coordinador vía WebSocket, recibe un bloque de filas para computar, ejecuta la evaluación de Mandelbrot y devuelve el resultado. |

### Flujo de Comunicación

Toda la comunicación coordinador-worker ocurre sobre **WebSocket** en el endpoint `/ws`.

```
Worker                              Coordinador
  │                                      │
  │──── WS Connect (/ws) ──────────────>│
  │──── Hello ─────────────────────────>│  Worker registrado, alias asignado
  │                                      │
  │<─── Compute {task_id, rows} ────────│  Tarea despachada
  │                                      │
  │  [computa Mandelbrot para las filas] │
  │                                      │
  │──── ComputeResult {task_id, data} ->│  Datos fusionados en el buffer
  │                                      │
  │<─── Compute {task_id, rows} ────────│  Siguiente tarea (si hay)
  │        ...                           │
  │                                      │
  │                               [todas las tareas listas]
  │                               guardar fractal.png
```

### Internos del Coordinador

El bucle principal del coordinador usa `tokio::select!` para manejar concurrentemente dos fuentes de eventos:

- **Tick de 1 segundo**: verifica tareas que superaron el timeout de 3 segundos y las re-encola; activa el inicio automático si pasaron 10 segundos desde la última conexión de un worker.
- **Canal `InternalMessage`**: recibe eventos de los handlers WebSocket de Axum (`WorkerConnected`, `WorkerDisconnected`, `WorkerFinished`).

Este diseño mantiene todo el estado mutable del coordinador en una única tarea async, evitando la sincronización de memoria compartida.

```
┌──────────────────────────────────────────────────┐
│              Bucle del Coordinador                │
│                                                   │
│  tokio::select! {                                 │
│    tick (1s) => verificar timeout + auto-inicio   │
│    rx.recv() => WorkerConnected                   │
│               | WorkerDisconnected                │
│               | WorkerFinished                    │
│  }                                                │
└──────────────────────────────────────────────────┘
         ▲                            ▲
         │  InternalMessage (mpsc)    │
┌────────┴────────┐        ┌──────────┴──────────┐
│  ws_handler #1  │        │   ws_handler #2      │
│  (tarea Axum)   │        │   (tarea Axum)       │
└─────────────────┘        └─────────────────────┘
```

### Internos del Worker

Cada worker ejecuta un bucle infinito de reconexión. Al conectarse:

1. Envía un mensaje `Hello`.
2. Espera comandos `Compute` en un bucle.
3. Al recibir un comando `Compute`, llama a `compute_block()` de forma síncrona y envía de vuelta `ComputeResult`.
4. Si la conexión se cae, espera 2 segundos y se reconecta automáticamente.

---

## 4. Estructura del Código

```
betmasters/
├── .github/
│   └── workflows/
│       └── gatekeeper.yml          # Gate de PR: main ← solo desde development
├── docker/
│   ├── .env.example
│   └── docker-compose.yml
├── docs/
│   ├── Configuracion Avanzada.md
│   ├── Docker compose.md
│   ├── Hub-and-Spoke.md
│   └── Virtual private network.md
├── kubernetes/
├── vpn/
└── rust/
    ├── Cargo.toml
    └── src/
        ├── main.rs                 # Punto de entrada; lee APP_ROLE, construye Coordinator o Worker
        ├── logger.rs               # Inicialización del logging
        ├── coordinator/
        │   ├── logic.rs            # Struct Coordinator, cola de tareas, bucle select!, guardado de imagen
        │   ├── handlers.rs         # Handlers HTTP + WebSocket de Axum
        │   └── types.rs            # CoordinatorCommand, WorkerMessage, InternalMessage, Task, WorkerState
        └── worker/
            ├── logic.rs            # Worker::run(), sesión WebSocket, bucle de reconexión
            ├── math.rs             # compute_block(), evaluate() — cómputo puro de Mandelbrot
            └── types.rs            # Definición del struct Worker
```

### Responsabilidades de los Módulos

**`main.rs`** — lee las variables de entorno y construye un `Coordinator` o un `Worker`. No vive aquí ninguna lógica de negocio; es puramente una capa de configuración y despacho.

**`coordinator/logic.rs`** — posee todo el estado mutable del coordinador: la cola de tareas pendientes (`VecDeque<Task>`), tareas asignadas (`HashMap<u32, Task>`), registro de workers (`HashMap<String, WorkerState>`), cola de workers inactivos, y el buffer de píxeles (`Vec<u32>`). Contiene el bucle de eventos `tokio::select!` y las funciones `save_image_impl` / `iter_to_color` para la salida PNG.

**`coordinator/handlers.rs`** — handler de upgrade WebSocket de Axum. Cada conexión lanza dos tareas concurrentes: `forward_commands` (envía frames JSON de `CoordinatorCommand` al worker) y `receive_results` (lee frames `WorkerMessage` y los reenvía como `InternalMessage` al bucle del coordinador vía mpsc).

**`coordinator/types.rs`** — todos los tipos de mensajes serializados sobre WebSocket (`CoordinatorCommand`, `WorkerMessage`) y el tipo del canal interno (`InternalMessage`). También define `Task` y `WorkerState`.

**`worker/logic.rs`** — implementa `Worker::run()` con el bucle de reconexión y `run_session()` para la sesión WebSocket activa.

**`worker/math.rs`** — cómputo puro: mapea coordenadas de píxel a coordenadas del plano complejo, evalúa la iteración de Mandelbrot para cada punto, y devuelve los conteos de iteración como `Vec<u32>`.

**`worker/types.rs`** — el struct `Worker` con la configuración (`coordinator_url`, `max_iters`, límites del plano complejo).

---

## 5. Algoritmo de Mandelbrot

### Base Matemática

El conjunto de Mandelbrot se define sobre el plano complejo. Para cada punto `c = x + yi`, iteramos la recurrencia:

```
z₀ = 0
z_{n+1} = z_n² + c
```

Un punto `c` se considera **fuera** del conjunto (escapa) si `|z_n| > 2` para algún `n` finito. Como `|z|² = a² + b²` (donde `a = Re(z)`, `b = Im(z)`), la condición de escape se verifica como `a² + b² > 4.0` — evitando una raíz cuadrada en cada iteración.

Si un punto alcanza `MAX_ITERS` iteraciones sin escapar, se considera **dentro** del conjunto y se renderiza en negro.

### Implementación (`math.rs`)

Para cada píxel en coordenadas de pantalla `(col, fila)`, la coordenada compleja es:

```
c_x = x_min + col * (x_max - x_min) / ancho
c_y = y_min + fila * (y_max - y_min) / alto
```

La iteración expande `z² + c` algebraicamente (evitando la sobrecarga de números complejos):

```
Re(z² + c) = a² - b² + c_x
Im(z² + c) = 2ab   + c_y
```

La función devuelve el conteo de iteraciones `i` en el que `a² + b² > 4`, o `MAX_ITERS` si el punto no escapó.

### Particionamiento en Bloques

El coordinador divide la imagen en bandas horizontales de `BLOCK_SIZE` filas al inicio:

```
Imagen (alto = H filas)
├── Tarea 0: filas   0 ..  99
├── Tarea 1: filas 100 .. 199
├── Tarea 2: filas 200 .. 299
│   ...
└── Tarea N: filas (H - resto) .. H
```

Cada tarea se envía a un worker. Los workers computan todos los píxeles de sus filas asignadas y devuelven un `Vec<u32>` plano de conteos de iteración. El coordinador escribe el resultado directamente en el offset correcto del buffer de almacenamiento.

### Colorización: Polinomios de Bernstein

Una vez completadas todas las tareas, el coordinador mapea cada conteo de iteraciones a un color RGB usando funciones de base de polinomios de Bernstein. Sea `t = iter / MAX_ITERS`:

```
R = 9.0  × (1-t) × t³  × 255
G = 15.0 × (1-t)² × t² × 255
B = 8.5  × (1-t)³ × t  × 255
```

Esto produce un gradiente suave de azul/verde oscuro (escape lento) a naranja/amarillo brillante (escape rápido), con negro para los puntos interiores (`iter == MAX_ITERS`). La forma polinomial garantiza continuidad y evita límites duros entre bandas de color.

---

## 6. Decisiones de Diseño

### Binario único con `APP_ROLE`

Un único binario de Rust compila tanto el coordinador como el worker. El rol se selecciona en tiempo de ejecución mediante la variable de entorno `APP_ROLE` (`coordinator` | `worker`).

**Por qué:** En un despliegue VPN, el mismo binario se distribuye a cada nodo. Cambiar el rol de un nodo requiere solo un cambio en la variable de entorno — sin recompilación, sin artefactos separados. Esto es especialmente práctico con Docker: una imagen, diferentes variables de entorno.

**Trade-off:** Ambos roles comparten el mismo árbol de dependencias, aumentando ligeramente el tamaño del binario.

### WebSocket vs HTTP polling

Los workers se conectan al endpoint `/ws` del coordinador y mantienen una conexión WebSocket persistente. El despacho de tareas y la entrega de resultados ocurren sobre esta única conexión.

**Por qué:** Con workers distribuidos en una VPN WireGuard, cada worker solo necesita conocer una URL: la IP VPN del coordinador. La conexión WebSocket es persistente y bidireccional — el coordinador puede enviar tareas al worker tan pronto como estén disponibles, sin la sobrecarga del polling. Esto también mapea naturalmente a la topología Hub-and-Spoke: todo el tráfico fluye a través del nodo coordinador.

**Trade-off:** WebSocket requiere que el coordinador mantenga una conexión abierta por worker. Para la escala de este proyecto (decenas de nodos) es insignificante.

### Canales `mpsc` entre handlers y el bucle del coordinador

Los handlers WebSocket de Axum corren como tareas async independientes. En lugar de darles acceso directo al estado mutable del coordinador (lo que requeriría `Arc<Mutex<Coordinator>>`), cada handler envía eventos al bucle del coordinador a través de un canal `mpsc` de Tokio.

**Por qué:** El estado del coordinador (cola de tareas, registro de workers, buffer de almacenamiento) se modifica en muchos lugares y en un orden específico. Un `Arc<Mutex<>>` funcionaría pero introduce contención de locks y riesgo de deadlocks. Con `mpsc`, todas las mutaciones de estado ocurren en una única tarea async en una secuencia bien definida, lo que es más simple de razonar y depurar.

**Trade-off:** El bucle del coordinador se convierte en un cuello de botella para las mutaciones de estado. Para esta carga de trabajo (un mensaje por tarea completada), no es una preocupación.

### `spawn_blocking` para guardar la imagen

Cuando todas las tareas se completan, el coordinador llama a `save_image_impl` vía `tokio::task::spawn_blocking`.

**Por qué:** Codificar un PNG de 3840×2160 implica trabajo significativo de CPU (iteración de píxeles, compresión PNG) e I/O bloqueante. Ejecutar esto directamente en el runtime async de Tokio bloquearía el event loop, impidiendo que el coordinador maneje cualquier otro mensaje. `spawn_blocking` lo descarga a un pool de hilos dedicado diseñado para operaciones bloqueantes.

---

## 7. Ejecución del Proyecto

Todos los comandos desde el directorio `rust/`.

### Build

```bash
cargo build
cargo build --release
```

### Ejecutar como Coordinador

```bash
APP_ROLE=coordinator \
PORT=8080 \
IMAGE_WIDTH=3840 \
IMAGE_HEIGHT=2160 \
BLOCK_SIZE=100 \
MAX_ITERS=1000 \
cargo run
```

### Ejecutar como Worker

```bash
APP_ROLE=worker \
PORT=8081 \
COORDINATOR_URL=http://10.10.10.1:8080 \
MAX_ITERS=1000 \
X_MIN=-2.0 \
X_MAX=1.0 \
Y_MIN=-1.5 \
Y_MAX=1.5 \
cargo run
```

Reemplaza `10.10.10.1` con la IP VPN WireGuard del coordinador. Los workers se reconectarán automáticamente si la conexión se cae.

### Variables de Entorno

| Variable | Rol | Default | Descripción |
|----------|-----|---------|-------------|
| `APP_ROLE` | ambos | `coordinator` | `coordinator` o `worker` |
| `PORT` | ambos | `8080` | Puerto de escucha (coordinador) o informativo (worker) |
| `COORDINATOR_URL` | worker | `http://127.0.0.1:8080` | URL HTTP completa del coordinador |
| `IMAGE_WIDTH` | coordinador | `3840` | Ancho de la imagen de salida en píxeles |
| `IMAGE_HEIGHT` | coordinador | `2160` | Alto de la imagen de salida en píxeles |
| `BLOCK_SIZE` | coordinador | `100` | Número de filas por tarea |
| `MAX_ITERS` | ambos | `1000` | Máximo de iteraciones de Mandelbrot |
| `X_MIN` / `X_MAX` | worker | `-2.0` / `1.0` | Límites horizontales del plano complejo |
| `Y_MIN` / `Y_MAX` | worker | `-1.5` / `1.5` | Límites verticales del plano complejo |
| `OUTPUT_FILE` | coordinador | `fractal.png` | Ruta del archivo PNG de salida |
| `BIND_ADDR` | coordinador | `0.0.0.0` | Dirección de bind del servidor HTTP |

### Ejecutar Tests

```bash
cargo test

# Ejecutar un test específico
cargo test <nombre_del_test>
```

---

## 8. Docker

Desde el directorio `docker/`:

```bash
cp .env.example .env
# Editar .env según sea necesario
docker compose up --build
```

---

## Flujo de Git

Los PRs a `main` deben originarse desde la rama `development`, aplicado por `.github/workflows/gatekeeper.yml`.
