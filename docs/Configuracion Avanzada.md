Configuraciones necesarias.

# 1. Persistencia de Conexión (`PersistentKeepalive`)

Por defecto, WireGuard es silencioso. Si no hay tráfico, el túnel se "duerme". Si tu servidor está detrás de un NAT o un Firewall estricto, la conexión podría cerrarse.

- **Configuración:** Añade esto en el bloque `[Peer]` de tus **clientes**.
- **Efecto:** Envía un paquete "latido" cada 25 segundos para mantener el túnel abierto.

```bash
[Peer]
PersistentKeepalive = 25
```

# 2. DNS Personalizado
Puedes obligar a tus clientes a usar un servidor DNS específico (como Pi-hole para bloquear publicidad, o Cloudflare por privacidad) en cuanto se conecten a la VPN.

- **Configuración:** Añade esto en el bloque `[Interface]` del **cliente**.

```bash
[Interface]
DNS = 1.1.1.1, 8.8.8.8
```
