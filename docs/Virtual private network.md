Implementacion de una vpn basica funcional. Si desea como mejorar la vpn acceda al siguiente archivo: [[Configuracion Avanzada]].
# 1. Instalación y Generación de Llaves
Instalar wireguard
```bash
sudo apt update && sudo apt install wireguard -y
```

Generar par de llaves 
```bash
umask 077 
wg genkey | tee privatekey | wg pubkey > publickey
```

# 2. Configuración de la Interfaz (`wg0.conf`)
Crea el archivo de configuración principal. Aquí definiremos la subred interna de la VPN y las reglas de red.

`sudo nano /etc/wireguard/wg0.conf`

Para que tu servidor WireGuard funcione correctamente, el archivo `wg0.conf` debe estructurarse en dos secciones principales: la **Interface** (el servidor) y los **Peers** (los clientes).

```bash
[Interface]
# La IP privada que tendrá el servidor dentro de la VPN
Address = 10.0.0.1/24

# El puerto UDP donde escuchará conexiones externas
ListenPort = 51820

# La llave privada generada en el servidor
PrivateKey = <TU_LLAVE_PRIVADA_SERVIDOR>

# --- Reglas de Enrutamiento ---
PostUp = iptables -A FORWARD -i %i -j ACCEPT; iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE
PostDown = iptables -D FORWARD -i %i -j ACCEPT; iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE

[Peer]
# Llave pública del cliente (celular, laptop, etc.)
PublicKey = <LLAVE_PUBLICA_DEL_CLIENTE>

# IP específica que se le asignará a este cliente
AllowedIPs = 10.0.0.2/32
```

### Entendiendo `PostUp` y `PostDown`

Estas directivas son comandos de shell que se ejecutan **automáticamente** justo después de que la interfaz sube (`PostUp`) o justo antes de que baje (`PostDown`).

En el contexto de una VPN, su función principal es el **Enrutamiento y NAT (Network Address Translation)**.

#### 1. `iptables -A FORWARD -i %i -j ACCEPT`

- **¿Qué hace?**: Permite que los paquetes que llegan a través de la interfaz de WireGuard (`%i`, que se expande a `wg0`) sean reenviados a otras interfaces (como tu salida a Internet).
- **Sin esto**: El servidor recibiría tus datos pero no sabría qué hacer con ellos; los descartaría por seguridad.
#### 2. `iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE`

- **¿Qué hace?**: Esta es la "magia" del enmascaramiento. Le dice al servidor: "Cuando un paquete salga hacia internet por la interfaz `eth0`, cambia su IP de origen (que es `10.0.0.x`) por la IP pública del servidor".
- **Por qué es vital**: Las IPs privadas de la VPN no son válidas en la internet pública. El servidor actúa como un intermediario que presta su identidad para que tú navegues.
#### 3. El uso de `PostDown`

- **¿Qué hace?**: Espejea los comandos de `PostUp` pero usando `-D` (Delete) en lugar de `-A` (Append).
- **Por qué es vital**: Limpia las reglas del firewall cuando apagas la VPN. Si no lo haces, cada vez que reinicies el servicio podrías terminar con cientos de reglas duplicadas en `iptables`, lo que ralentiza la red y ensucia la configuración.
### Resumen de variables

- **`%i`**: Es un comodín que WireGuard reemplaza por el nombre del archivo (en este caso, `wg0`).
- **`eth0`**: Debes asegurarte de que este sea el nombre de tu interfaz física. Puedes verificarlo corriendo el comando `ip route | grep default`.
# 3. Habilitar el Reenvío de IP (IP Forwarding)

Para que el servidor actúe como un túnel y pase el tráfico de los clientes hacia internet, debemos habilitar esta función en el kernel:

```bash
echo "net.ipv4.ip_forward=1" | sudo tee -a /etc/sysctl.conf
sudo sysctl -p
```
# 4. Configuración del Firewall (UFW)

WireGuard utiliza **UDP**. Debes abrir el puerto que configuramos anteriormente:

```bash
sudo ufw allow 51820/udp
sudo ufw allow OpenSSH
sudo ufw enable
```
# 5. Levantar el Servicio
Utilizamos `wg-quick` para iniciar la interfaz y habilitarla para que inicie automáticamente tras un reinicio.

```bash
sudo wg-quick up wg0
sudo systemctl enable wg-quick@wg0
```

# 6. Añadir un Cliente (Peer)

Para conectar un dispositivo (como tu laptop o móvil), debes generar llaves para el cliente y añadirlas al servidor.

**En el Servidor:** `sudo nano /etc/wireguard/wg0.conf` Añade al final del archivo:

```bash
[Peer]
PublicKey = <LLAVE_PUBLICA_DEL_CLIENTE>
AllowedIPs = 10.0.0.2/32
```

Luego reinicia la interfaz: `sudo wg-quick down wg0 && sudo wg-quick up wg0`.
