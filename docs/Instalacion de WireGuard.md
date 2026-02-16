# Configuración de WireGuard VPN

Este documento detalla los pasos para instalar y configurar un servidor VPN utilizando WireGuard en un sistema basado en Debian/Ubuntu.

## 1. Instalación de Paquetes
Primero, actualizamos el sistema e instalamos WireGuard junto con herramientas útiles como UFW (firewall) e iperf3 (para pruebas de rendimiento).

```bash
# Actualizar repositorios y paquetes
sudo apt update && sudo apt upgrade -y

# Instalar WireGuard, UFW e iperf3
sudo apt install -y wireguard ufw iperf3
```

> **Nota:** Se recomienda mantener `iperf3` activo para realizar pruebas de velocidad de la conexión VPN.

## 2. Generación de Llaves
WireGuard utiliza un par de llaves criptográficas (pública y privada) para asegurar la conexión.

```bash
# Asegurar que los archivos creados solo sean legibles por el usuario actual
umask 077

# Generar la llave privada y derivar la llave pública
wg genkey | tee privatekey | wg pubkey > publickey
```

Puedes ver el contenido de las llaves con:
```bash
nano privatekey
nano publickey
```

## 3. Identificación de la Interfaz de Red
Es crucial identificar el nombre de la interfaz de red física del servidor (por ejemplo, `enp0s3`, `eth0` o `ens33`) para configurar correctamente el enmascaramiento de red (NAT).

```bash
ip route
```
Busca la interfaz asociada a la ruta por defecto.

## 4. Habilitar el Reenvío de IP (IP Forwarding)
Para que el servidor pueda redirigir el tráfico de los clientes hacia Internet, debemos habilitar el reenvío de paquetes IPv4.

```bash
# Activar inmediatamente
sudo sysctl -w net.ipv4.ip_forward=1

# Hacer el cambio permanente tras reinicios
echo "net.ipv4.ip_forward=1" | sudo tee -a /etc/sysctl.conf
```

## 5. Configuración del Archivo `wg0.conf`
Crea el archivo de configuración en `/etc/wireguard/wg0.conf`.

```bash
sudo nano /etc/wireguard/wg0.conf
```

Copia y pega la siguiente estructura, reemplazando `<TU_LLAVE_PRIVADA_AQUÍ>` y ajustando el nombre de la interfaz (`enp0s3`) si es necesario:

```ini
[Interface]
PrivateKey = <TU_LLAVE_PRIVADA_AQUÍ>
Address = 10.0.0.1/24
ListenPort = 51820

# Reglas para que UFW y el tráfico fluyan (NAT)
PostUp = ufw route allow in on wg0 out on enp0s3
PostUp = iptables -t nat -I POSTROUTING -o enp0s3 -j MASQUERADE
PostDown = ufw route delete allow in on wg0 out on enp0s3
PostDown = iptables -t nat -D POSTROUTING -o enp0s3 -j MASQUERADE
```

## 6. Levantar la Interfaz
Una vez configurado, puedes iniciar el servicio de la VPN.

### Activar la interfaz manualmente
Para levantar el servicio de inmediato, usa el comando `wg-quick`:

```bash
sudo wg-quick up wg0
```

### Habilitar para que inicie con el sistema
Para asegurar que la VPN se active automáticamente al encender el servidor:

```bash
sudo systemctl enable wg-quick@wg0
```

### Iniciar el servicio vía systemctl
Si prefieres usar la gestión de servicios estándar (y no lo hiciste con `wg-quick` arriba):

```bash
sudo systemctl start wg-quick@wg0
```

## 7. ¿Cómo saber si realmente está activo?
Ahora sí, los comandos de verificación deben darte resultados positivos:

*   **`sudo wg show`**: Te mostrará el nombre de la interfaz, la llave pública del servidor y el puerto de escucha. Si hay clientes conectados, también verás su información.
*   **`ip addr show wg0`**: Verás la interfaz virtual creada con la IP `10.0.0.1`.
*   **`sudo systemctl status wg-quick@wg0`**: Te confirmará con un círculo verde que el servicio está `active (exited)`. 
    > **Nota:** Es normal que diga "exited" porque `wg-quick` es un script que se ejecuta y termina tras configurar los parámetros en el kernel.
