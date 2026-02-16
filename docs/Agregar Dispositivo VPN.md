# Agregar un Dispositivo a la VPN (WireGuard)

Este proceso describe cómo configurar un nuevo cliente (por ejemplo, un teléfono móvil) y registrarlo en el servidor VPN.

## 1. Generar llaves para el nuevo dispositivo
Cada dispositivo necesita su propio par de llaves. Realiza esto en una terminal:

```bash
umask 077
wg genkey | tee phone_private | wg pubkey > phone_public
```

## 2. Crear el archivo de configuración del cliente
Crea un archivo llamado `phone.conf` en tu carpeta personal. Este archivo contendrá la configuración necesaria para que el dispositivo se conecte al servidor.

```ini
[Interface]
PrivateKey = <CONTENIDO_DE_phone_private>
Address = 10.0.0.2/24
DNS = 1.1.1.1

[Peer]
PublicKey = <LLAVE_PÚBLICA_DEL_SERVIDOR_VM>
Endpoint = <IP_PUBLICA_O_LOCAL_DE_TU_VM>:51820
AllowedIPs = 10.0.0.0/24
```

> **Nota:** Reemplaza `<CONTENIDO_DE_phone_private>` con el texto dentro del archivo `phone_private`, `<LLAVE_PÚBLICA_DEL_SERVIDOR_VM>` con la llave pública del servidor, y `<IP_PUBLICA_O_LOCAL_DE_TU_VM>` con la dirección IP de tu servidor.

## 3. Registrar el dispositivo como "Peer" en el servidor
El servidor debe autorizar al nuevo dispositivo para permitirle la conexión.

### Registro temporal (inmediato)
Ejecuta este comando en el servidor (reemplaza con la llave pública del teléfono):

```bash
sudo wg set wg0 peer <CONTENIDO_DE_phone_public> allowed-ips 10.0.0.2
```

### Registro permanente
Para que la configuración persista tras reiniciar el servidor, edita el archivo `/etc/wireguard/wg0.conf` y añade al final:

```ini
[Peer]
PublicKey = <CONTENIDO_DE_phone_public>
AllowedIPs = 10.0.0.2/32
```

## 4. Transferir la configuración mediante Código QR
La forma más sencilla de pasar esta configuración a un teléfono móvil es generando un código QR que puedes escanear con la app de WireGuard.

```bash
# Instalar la herramienta qrencode
sudo apt install qrencode -y

# Generar el código QR en la terminal
qrencode -t ansiutf8 < phone.conf
```
