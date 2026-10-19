### Novedades de la 4.1
- **Inventario y limiteds:** ya no se queda con "datos incompletos". Lee el inventario con la sesión de cada cuenta y espera y reintenta cuando Roblox limita las peticiones.
- **Multi Roblox:** si hay un Roblox abierto que lo bloquea, la app ofrece cerrarlo y activarlo. Pulsa "(bloqueado)" o guarda los ajustes.
- **Ajustes:** ya se guardan aunque Multi Roblox no se pueda activar.
- **ram.log:** cada línea lleva la fecha, la hora local y el desfase UTC (por ejemplo `2026-10-19 19:59:15 UTC-03:00`).
- El Place ID, el Job ID y el usuario a seguir se recuerdan entre sesiones (también los de la 3.x).
- Español por defecto, "1 aviso" en singular, el contador de amigos en línea incluye a los que están en juego y los servidores sin ping muestran "—".

### Descargas
- **Portable.zip:** descomprime en una carpeta y abre `Roblox Account Manager.exe`. Tus datos se guardan en esa misma carpeta.
- **Setup.exe:** instalador. Se instala para tu usuario, sin pedir permisos de administrador.

Necesita Windows 10 u 11 y el WebView2 Runtime de Microsoft (ya viene con Windows 11; el instalador lo descarga si falta).

### Aviso de Windows
El programa no está firmado, así que Windows SmartScreen puede mostrar "Windows protegió tu PC". Pulsa **Más información → Ejecutar de todas formas**.

Reporta lo que falle en Issues, con capturas y el archivo `ram.log` que está junto al ejecutable.
