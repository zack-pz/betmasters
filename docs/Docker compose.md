Para levantar un servidor WireGuard usando **Docker Compose**, la mejor opción es utilizar la imagen de **LinuxServer.io**, ya que es la más estable, está bien documentada y facilita enormemente la gestión de llaves y clientes.
### 1. Preparación del Entorno

Primero, asegúrate de tener instalado el módulo de WireGuard en tu host (Ubuntu Server). Aunque Docker lo gestiona, el kernel debe soportarlo:

```bash
sudo apt update && sudo apt install wireguard-dkms -y
```
### 2. El archivo `docker-compose.yml`

Crea un directorio para el proyecto y dentro el archivo de configuración:

```yaml
services:
  wireguard:
    image: lscr.io/linuxserver/wireguard:latest
    container_name: wireguard
    cap_add:
      - NET_ADMIN
      - SYS_MODULE
    environment:
      - PUID=1000
      - PGID=1000
      - TZ=America/Mexico_City
      - SERVERURL=tu_ip_publica_o_dominio # Ejemplo: 201.145.x.x
      - SERVERPORT=51820
      - PEERS=laptop,celular,tablet      # Nombres de los clientes a generar
      - PEERDNS=1.1.1.1                 # DNS que usarán los clientes
      - INTERNAL_SUBNET=10.13.13.0      # Subred de la VPN
      - ALLOWEDIPS=0.0.0.0/0            # Túnel total (navegación segura)
      - LOG_CONFS=true                  # Para ver los QR en los logs
    volumes:
      - ./config:/config
      - /lib/modules:/lib/modules       # Fundamental para el acceso al Kernel
    ports:
      - 51820:51820/udp
    sysctls:
      - net.ipv4.conf.all.src_valid_mark=1
      - net.ipv4.ip_forward=1           # Habilita el ruteo
    restart: unless-stopped
```

---
### 3. Despliegue y Configuración de Clientes

Levanta el contenedor con el siguiente comando:

```bash
docker compose up -d
```

Una vez que el contenedor esté corriendo, la magia de esta imagen es que **ya generó todo por ti**. Tienes dos formas de obtener la configuración para tus dispositivos:
#### A. Mediante Códigos QR (Ideal para móviles)

Ejecuta el siguiente comando para ver los códigos QR directamente en tu terminal:

```bash
docker logs -f wireguard
```

_Escanea el código con la app de WireGuard en tu iPhone o Android._

#### B. Mediante archivos de configuración (Ideal para PC/Laptop)

Docker habrá creado una estructura de carpetas en `./config`. Entra y verás carpetas para cada "Peer":
- `./config/peer_laptop/peer_laptop.conf`
- `./config/peer_celular/peer_celular.conf`

Solo tienes que copiar ese archivo `.conf` a tu laptop e importarlo en el cliente de WireGuard.

---
### 4. Diagnóstico y Pruebas

Como mencionaste herramientas de red, aquí es donde las usas para validar que todo esté en orden:

- **Verificar túnel:** Dentro del contenedor, usa `wg show` para ver handshakes activos.

    ```bash
    docker exec -it wireguard wg show
    ```
    
- **Prueba de ancho de banda:** Puedes levantar un contenedor de `iperf3` unido a la misma red de Docker para medir el throughput real del túnel.

- **Logs del Firewall:** Si no conectas, revisa `iptables` en el host para asegurar que el puerto `51820/udp` esté aceptando tráfico.

---
