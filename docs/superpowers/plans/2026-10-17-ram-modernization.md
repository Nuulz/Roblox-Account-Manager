# RAM Modernization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fork Roblox Account Manager, eliminar las causas de detección por antivirus, modernizar la UI a WPF oscuro, y mejorar el login para evitar detección de bot por Roblox.

**Architecture:** Migrar el proyecto de .NET Framework 4.7.2 + WinForms + CefSharp a .NET 8 + WPF + WebView2. La lógica de negocio (Account, RobloxProcess, WebServer, Nexus, etc.) se reutiliza directamente; solo el proyecto, la UI y el browser embebido cambian. Sin Costura.Fody ni PuppeteerSharp — ambos son los principales triggers de AV.

**Tech Stack:** C# 12, .NET 8 (net8.0-windows), WPF, Microsoft.Web.WebView2, ModernWpf.UI, Newtonsoft.Json, RestSharp, websocket-sharp, libsodium-net, SharpZipLib.

**Spec:** `docs/superpowers/specs/` (diseño aprobado en conversación 2026-10-17)

## Global Constraints

- Target framework: `net8.0-windows` con `<UseWPF>true</UseWPF>`
- NO Costura.Fody — usar `<PublishSingleFile>true</PublishSingleFile>` en Release
- NO CefSharp — reemplazar con `Microsoft.Web.WebView2` (≥ 1.0.2210)
- NO PuppeteerSharp / PuppeteerExtraSharp — eliminados completamente
- NO post-build que sobrescriba el EXE con un binario Launcher externo
- Preservar todos los archivos de lógica: Account.cs, RobloxProcess.cs, WebServer.cs, Nexus/*, Cryptography.cs, GamePlayer.cs, etc.
- Namespace raíz: `RBX_Alt_Manager` (sin cambios, para no romper serialización JSON de cuentas guardadas)
- Los datos de cuentas (accounts.json) deben ser retrocompatibles
- Windows 10+ únicamente (WebView2 runtime requiere Windows 10)

## Review Focus

- **Login con 2FA activado:** el WebView2 debe poder mostrar el formulario de autenticación de dos factores sin cerrarse antes de que el usuario lo complete.
- **Cookie extraction tras login:** hay que leer `.ROBLOSECURITY` del WebView2 después de que Roblox redirija al home; si la cookie no está lista aún, el flujo debe reintentar.
- **Multi-instancia con .NET 8:** `RobloxProcess.cs` usa log file watching; verificar que los paths de logs de Roblox no hayan cambiado.
- **Accounts.json cargado con namespace antiguo:** el JSON fue serializado con `RBX_Alt_Manager.Account`; si el namespace cambia, `JsonConvert.DeserializeObject<List<Account>>` falla silenciosamente.
- **WebView2 runtime no instalado:** si el usuario no tiene el runtime de Edge WebView2, el app debe mostrar un mensaje claro y link de descarga, no un crash.

---

### Task 1: Nuevo proyecto SDK-style (.NET 8 + WPF)

**Files:**
- Create: `RBX Alt Manager/RBX Alt Manager.csproj` (reemplaza el existente)
- Create: `RBX Alt Manager/App.xaml`
- Create: `RBX Alt Manager/App.xaml.cs`

**Interfaces:**
- Produces: proyecto compilable que referencia todos los archivos `.cs` de lógica existentes

- [ ] **Step 1: Reemplazar el .csproj con SDK-style**

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>WinExe</OutputType>
    <TargetFramework>net8.0-windows</TargetFramework>
    <UseWPF>true</UseWPF>
    <AssemblyName>Roblox Account Manager</AssemblyName>
    <RootNamespace>RBX_Alt_Manager</RootNamespace>
    <Nullable>disable</Nullable>
    <ImplicitUsings>disable</ImplicitUsings>
    <ApplicationIcon>Resources\team_KX4_icon.ico</ApplicationIcon>
    <PublishSingleFile Condition="'$(Configuration)'=='Release'">true</PublishSingleFile>
    <SelfContained>false</SelfContained>
    <RuntimeIdentifier>win-x64</RuntimeIdentifier>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Microsoft.Web.WebView2" Version="1.0.2210.55" />
    <PackageReference Include="ModernWpf.UI" Version="0.9.6" />
    <PackageReference Include="Newtonsoft.Json" Version="13.0.3" />
    <PackageReference Include="RestSharp" Version="110.2.0" />
    <PackageReference Include="WebSocketSharp-netstandard" Version="1.0.1" />
    <PackageReference Include="SharpZipLib" Version="1.4.2" />
    <PackageReference Include="libsodium-net" Version="0.10.0" />
    <PackageReference Include="Esendex.TokenBucket" Version="1.0.7" />
  </ItemGroup>
  <ItemGroup>
    <Resource Include="Resources\**\*.*" />
  </ItemGroup>
</Project>
```

- [ ] **Step 2: Crear App.xaml mínimo**

```xml
<Application x:Class="RBX_Alt_Manager.App"
             xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
             xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
             xmlns:ui="http://schemas.modernwpf.com/2019"
             StartupUri="MainWindow.xaml">
  <Application.Resources>
    <ResourceDictionary>
      <ResourceDictionary.MergedDictionaries>
        <ui:ThemeResources RequestedTheme="Dark"/>
        <ui:XamlControlsResources/>
      </ResourceDictionary.MergedDictionaries>
    </ResourceDictionary>
  </Application.Resources>
</Application>
```

- [ ] **Step 3: Crear App.xaml.cs**

Arrancar con `[STAThread]` y llamar a `AccountManager.Initialize()` antes de `new Application().Run()`. Manejar `WebView2 runtime not found` en `AppDomain.CurrentDomain.UnhandledException`.

- [ ] **Step 4: Verificar que el proyecto compila (solo lógica, sin UI)**

```
dotnet build "RBX Alt Manager/RBX Alt Manager.csproj" -c Debug
```
Esperado: errores de tipos WinForms (normales en este paso), pero NO errores de NuGet ni de proyecto.

- [ ] **Step 5: Commit**

```
git add "RBX Alt Manager/RBX Alt Manager.csproj" "RBX Alt Manager/App.xaml" "RBX Alt Manager/App.xaml.cs"
git commit -m "chore: migrate to .NET 8 SDK-style WPF project, remove Costura.Fody and CefSharp"
```

---

### Task 2: Eliminar dependencias AV-trigger y adaptar lógica

**Files:**
- Modify: `RBX Alt Manager/Classes/AccountBrowser.cs`
- Delete (referencias): PuppeteerSharp, PuppeteerExtraSharp, CefSharp, Costura.Fody, Yove.Proxy (no usada en lógica core), log4net, ObjectListView, FastColoredTextBox, WindowsAPICodePack

**Interfaces:**
- Consumes: csproj limpio de Task 1
- Produces: `AccountBrowser` sin CefSharp/PuppeteerSharp; browser embebido manejado por WebView2 en Task 3

- [ ] **Step 1: Reescribir AccountBrowser.cs sin PuppeteerSharp ni CefSharp**

La clase mantiene la misma interfaz pública pero usa `Microsoft.Web.WebView2.Wpf.WebView2` como tipo de browser. En este task, la clase solo debe:
- Exponer `OpenBrowser(string url, string cookie)` que lanza una ventana WPF con WebView2
- Exponer `ExtractCookie(string domain, string name) -> Task<string>` que llama a `webView.CoreWebView2.CookieManager`
- Eliminar todo uso de `PuppeteerExtraSharp`, `BrowserFetcher`, `Page`, `Browser`

- [ ] **Step 2: Eliminar using statements de CefSharp y PuppeteerSharp en todos los archivos**

```
grep -r "CefSharp\|PuppeteerSharp\|PuppeteerExtraSharp\|Yove.Proxy\|log4net" "RBX Alt Manager/" --include="*.cs" -l
```
Para cada archivo encontrado: remover el `using` y el código que lo usa (reemplazar con stub o WebView2 equivalente).

- [ ] **Step 3: Remover el post-build event del Launcher**

En el nuevo .csproj no hay post-build event. El binario `Launcher` en el repo no se usa — verificar con `git log -- Launcher` qué hace. Si es solo un loader trampolín, eliminar completamente.

- [ ] **Step 4: Compilar y verificar que no hay referencias a CefSharp/Puppeteer**

```
dotnet build "RBX Alt Manager/RBX Alt Manager.csproj" -c Debug 2>&1 | grep -i "cef\|puppeteer\|costura"
```
Esperado: sin output (ninguna referencia encontrada).

- [ ] **Step 5: Commit**

```
git add "RBX Alt Manager/Classes/AccountBrowser.cs"
git commit -m "refactor: replace CefSharp+PuppeteerSharp with WebView2 stub, remove AV-trigger dependencies"
```

---

### Task 3: Ventana principal WPF (layout del mockup)

**Files:**
- Create: `RBX Alt Manager/MainWindow.xaml`
- Create: `RBX Alt Manager/MainWindow.xaml.cs`
- Create: `RBX Alt Manager/ViewModels/AccountViewModel.cs`
- Modify: `RBX Alt Manager/App.xaml` (StartupUri apunta a MainWindow.xaml)

**Interfaces:**
- Consumes: `Account` (Classes/Account.cs), `AccountManager` static class
- Produces: ventana con dos paneles, lista de cuentas y panel de detalle/acciones

Layout del mockup:
```
┌─────────────────────────────────┬──────────────────────────────┐
│  [Username] [Alias] [Desc]      │  Title | Settings  ListView  │
│  ─────────────────────────────  │  PlaceID  [________] [copy]  │
│  GakuranDev  Gakuran  Main...   │  JobID    [________] [copy]  │
│  shadow_fox  Shadow   Alt...    │                               │
│                                 │  Username [________] [Rename] │
│                                 │  Alias    [________] [SetAls] │
│                                 │  Description                  │
│                                 │  [__________________]         │
│                                 │  [Join Server][Utils][Follow] │
│                                 │  [     Account Utilities    ] │
├─────────────────────────────────┴──────────────────────────────┤
│ [+Add] [-Remove] [☑HideUser] [🌐OpenBrowser▼] [⬜] [EditTheme] │
└────────────────────────────────────────────────────────────────┘
```

- [ ] **Step 1: Crear AccountViewModel.cs**

Propiedades: `string Username`, `string Alias`, `string Description`, `string Group`. Implementar `INotifyPropertyChanged`. Constructor que toma `Account account`.

- [ ] **Step 2: Crear MainWindow.xaml**

Estructura principal:
- `Grid` de dos columnas: izquierda 65% (lista), derecha 35% (detalle)
- Columna izquierda: `ListView` con columnas Username, Alias, Description. Items source = `ObservableCollection<AccountViewModel>`.
- Columna derecha: `StackPanel` con los campos del mockup (PlaceID, JobID, Username, Alias, Description, botones Join Server / Utilities / Follow / Account Utilities).
- Fila inferior: barra de herramientas con Add, Remove, Hide Usernames, Open Browser (SplitButton), Edit Theme, Account Control.
- Colores base: fondo `#1a1a2e` (casi negro azulado), acento `#2d5a8e` (azul medio). Todos los controles via `ModernWpf`.

- [ ] **Step 3: Crear MainWindow.xaml.cs**

Conectar: selección en ListView → poblar panel derecho. Botones Add/Remove → `AccountManager.AddAccount()` / `AccountManager.RemoveAccount()`. Join Server → `AccountManager.JoinServer()`. PlaceID y JobID leen de `AccountManager.PlaceID` y `AccountManager.JobID` (campos estáticos existentes).

- [ ] **Step 4: Compilar y verificar que la ventana abre**

```
dotnet run --project "RBX Alt Manager/RBX Alt Manager.csproj"
```
Esperado: ventana oscura de dos paneles, lista vacía, sin excepciones.

- [ ] **Step 5: Test — Review Focus: WebView2 runtime no instalado**

En un entorno sin WebView2 runtime, el constructor de `WebView2` lanza `WebView2RuntimeNotFoundException`. En App.xaml.cs el handler de `UnhandledException` debe mostrar `MessageBox` con texto: "WebView2 runtime requerido. Descárgalo en https://developer.microsoft.com/microsoft-edge/webview2/" y cerrar la app limpiamente.

- [ ] **Step 6: Commit**

```
git add "RBX Alt Manager/MainWindow.xaml" "RBX Alt Manager/MainWindow.xaml.cs" "RBX Alt Manager/ViewModels/AccountViewModel.cs"
git commit -m "feat: add WPF dark two-panel main window with ModernWpf"
```

---

### Task 4: Migrar forms secundarios de WinForms a WPF

**Files:**
- Create: `RBX Alt Manager/Views/LoginWindow.xaml` + `.cs`
- Create: `RBX Alt Manager/Views/ImportWindow.xaml` + `.cs`
- Create: `RBX Alt Manager/Views/ServerListWindow.xaml` + `.cs`
- Create: `RBX Alt Manager/Views/SettingsWindow.xaml` + `.cs`
- Delete: todos los `Forms/*.cs` + `Forms/*.Designer.cs` de WinForms (AccountFields, AccountUtils, ArgumentsForm, ImportForm, ServerList, SettingsForm, ThemeEditor, Updater, RecentGamesForm, MissingAssets)

**Interfaces:**
- Consumes: lógica de las clases existentes (Server.cs, GamePlayer.cs, Utilities.cs, IniFile.cs)
- Produces: ventanas WPF que reemplazan 1:1 las Forms originales

- [ ] **Step 1: Crear LoginWindow.xaml con WebView2 embebido**

La ventana contiene un `<wv2:WebView2>` que navega a `https://www.roblox.com/login`. Después de que la URL cambie a `https://www.roblox.com/` (home), llamar a `CoreWebView2.CookieManager.GetCookiesAsync(".roblox.com")` y extraer `.ROBLOSECURITY`.

- [ ] **Step 2: Test — Review Focus: Login con 2FA**

Navegar a login, ingresar credenciales, verificar que si Roblox muestra el formulario 2FA la ventana permanece abierta y no extrae la cookie hasta que la URL llegue a `roblox.com/home` o `roblox.com/` con `?rbxid=` ausente.

- [ ] **Step 3: Test — Review Focus: Cookie extraction timing**

Después del redirect post-login, añadir un retry loop de hasta 3 segundos en `ExtractCookie`: si `.ROBLOSECURITY` aún no aparece, esperar 500ms y reintentar hasta 6 veces.

- [ ] **Step 4: Crear ImportWindow, ServerListWindow, SettingsWindow**

Cada una porta la lógica del Form original a WPF XAML. El look sigue el mismo esquema oscuro (fondo `#1a1a2e`, texto `#e0e0e0`, acento `#2d5a8e`).

- [ ] **Step 5: Compilar y verificar que todos los Forms WinForms están eliminados**

```
dotnet build "RBX Alt Manager/RBX Alt Manager.csproj" -c Debug 2>&1 | grep -i "winforms\|System.Windows.Forms"
```
Esperado: sin referencias a `System.Windows.Forms`.

- [ ] **Step 6: Commit**

```
git add "RBX Alt Manager/Views/" 
git commit -m "feat: replace WinForms dialogs with WPF windows, WebView2 login with cookie extraction"
```

---

### Task 5: Migrar AccountManager.cs (main form) a lógica pura

**Files:**
- Modify: `RBX Alt Manager/AccountManager.cs` (separar lógica de UI)
- Delete: `RBX Alt Manager/AccountManager.Designer.cs`

**Interfaces:**
- Consumes: `MainWindow` de Task 3
- Produces: `AccountManager` como clase estática de lógica sin referencias a WinForms

- [ ] **Step 1: Identificar qué en AccountManager.cs es lógica vs. UI**

```
grep -n "this\.\|Controls\.\|ListBox\.\|DataGridView\.\|Form\." "RBX Alt Manager/AccountManager.cs" | head -50
```

- [ ] **Step 2: Extraer lógica a AccountManager.cs (clase estática)**

Mover: `SaveAccounts()`, `LoadAccounts()`, `JoinServer()`, `LaunchRoblox()`, `RefreshCookie()`, `AddAccount()`, `RemoveAccount()`, settings load/save. Todos los métodos que toquen controles WinForms se mueven a `MainWindow.xaml.cs`.

- [ ] **Step 3: Test — Review Focus: accounts.json retrocompatible**

Crear un `accounts.json` de prueba con el formato del original (namespace `RBX_Alt_Manager.Account`). Llamar `AccountManager.LoadAccounts()` y verificar que la lista no está vacía y que `account.Username` no es null.

```csharp
[Test]
public void LoadAccounts_ExistingJson_DeserializesCorrectly()
{
    File.WriteAllText("accounts.json", "[{\"Username\":\"testuser\",\"UserID\":123,\"SecurityToken\":\"tok\",\"Group\":\"Default\"}]");
    AccountManager.LoadAccounts();
    Assert.IsTrue(AccountManager.Accounts.Count == 1);
    Assert.AreEqual("testuser", AccountManager.Accounts[0].Username);
}
```

- [ ] **Step 4: Compilar release**

```
dotnet publish "RBX Alt Manager/RBX Alt Manager.csproj" -c Release -r win-x64 --self-contained false
```
Esperado: un solo EXE en `build/Release/` sin carpeta de DLLs sueltas.

- [ ] **Step 5: Commit**

```
git add "RBX Alt Manager/AccountManager.cs"
git commit -m "refactor: separate AccountManager business logic from WPF UI"
```

---

### Task 6: Verificación final — AV y build limpio

**Files:**
- No changes; solo verificación

**Interfaces:**
- Consumes: EXE de Task 5

- [ ] **Step 1: Verificar ausencia de firmas AV conocidas en el binario**

```
strings "build/Release/Roblox Account Manager.exe" | grep -i "puppeteer\|cefsharp\|costura\|fody"
```
Esperado: sin output.

- [ ] **Step 2: Subir a VirusTotal (paso manual)**

Subir `build/Release/Roblox Account Manager.exe` a virustotal.com. Meta: <5 detecciones (las que quedan serán falsos positivos genéricos de tools de Roblox).

- [ ] **Step 3: Verificar que la app abre y carga cuentas existentes**

Correr el EXE, verificar: ventana oscura abre, cuentas del `accounts.json` real aparecen en la lista, seleccionar una cuenta muestra sus detalles en el panel derecho.

- [ ] **Step 4: Verificar login con WebView2**

Usar "Add Account" → Login Window → ingresar credenciales → verificar que la cookie se extrae y la cuenta aparece en la lista.

- [ ] **Step 5: Tag de release**

```
git tag v4.0.0-modernized
```
