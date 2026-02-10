Para implementar correctamente una topología **Hub-and-Spoke** (Estrella) en WireGuard, el servidor Ubuntu actuará como el **Hub** (centro) y todos los demás dispositivos como **Spokes** (radios).

En esta arquitectura, los Spokes no se conectan entre sí directamente, sino que todo el tráfico pasa por el Hub. Esto es ideal para centralizar la seguridad y el monitoreo.

### 1. Configuración del Hub (El Servidor)

El archivo `wg0.conf` en el servidor debe estar preparado para "rutear" el tráfico que viene de un Spoke hacia otro.

`sudo nano /etc/wireguard/wg0.conf`

```bash
[Interface]
Address = 10.0.0.1/24
ListenPort = 51820
PrivateKey = <LLAVE_PRIVADA_HUB>

# ACTIVAR EL RUTEO INTERNO
# Estas reglas permiten que el tráfico que entra por wg0 vuelva a salir por wg0 hacia otro spoke
PostUp = iptables -A FORWARD -i %i -o %i -j ACCEPT; iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE
PostDown = iptables -D FORWARD -i %i -o %i -j ACCEPT; iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE

# Spoke A (Laptop)
[Peer]
PublicKey = <LLAVE_PUBLICA_SPOKE_A>
AllowedIPs = 10.0.0.2/32

# Spoke B (Servidor en AWS)
[Peer]
PublicKey = <LLAVE_PUBLICA_SPOKE_B>
AllowedIPs = 10.0.0.3/32
```

---

### 2. Configuración de los Spokes (Los Clientes)

Para que los Spokes puedan verse entre sí, su configuración de `AllowedIPs` debe incluir el rango completo de la VPN, no solo la IP del servidor.

**Archivo en Spoke A (10.0.0.2):**

```bash
[Interface]
Address = 10.0.0.2/32
PrivateKey = <LLAVE_PRIVADA_SPOKE_A>

[Peer]
PublicKey = <LLAVE_PUBLICA_HUB>
Endpoint = <IP_PUBLICA_HUB>:51820
# IMPORTANTE: Incluir la subred completa para que sepa que el tráfico a 10.0.0.3 debe ir por el túnel
AllowedIPs = 10.0.0.0/24 
PersistentKeepalive = 25
```

---

### 3. Requisitos de Red Críticos

Para que esta metodología funcione "correctamente", debes asegurar tres puntos en el Hub:

1. **IP Forwarding:** El kernel de Linux debe tener permitido mover paquetes entre interfaces.

``` bash
sudo sysctl -w net.ipv4.ip_forward=1
```

2. **Regla Forward en IPTables:** Como viste en el `PostUp`, la clave es `-A FORWARD -i wg0 -o wg0 -j ACCEPT`. Por defecto, Docker o UFW a veces bloquean el tráfico que entra y sale por la misma interfaz. Esta regla lo permite explícitamente.

3. **Firewall del Host:** Si usas **UFW**, debes permitir el tráfico de reenvío:
    - Edita `/etc/default/ufw` y cambia `DEFAULT_FORWARD_POLICY="ACCEPT"`.
    - Luego ejecuta `sudo ufw reload`.

---

### 4. Verificación de la Topología

Una vez configurado, puedes validar la ruta desde el **Spoke A** hacia el **Spoke B**:

```bash
# En el Spoke A
ping 10.0.0.3
traceroute 10.0.0.3
```

Deberías ver que el primer salto es `10.0.0.1` (el Hub) y el segundo es `10.0.0.3`.
### ¿Por qué hacerlo así?

- **Centralización:** Puedes poner un IDS (como Suricata) o un Firewall en el Hub para inspeccionar todo el tráfico entre sucursales o dispositivos.
- **Simplicidad en AWS:** Como usas `us-east-1` o `us-west-2`, puedes tener el Hub en una instancia EC2 y conectar tu oficina en México y tus dispositivos móviles sin abrir puertos en cada uno de ellos, solo en el Hub.