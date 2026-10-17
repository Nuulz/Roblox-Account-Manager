using Microsoft.Web.WebView2.Core;
using Microsoft.Web.WebView2.WinForms;
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Numerics;
using System.Threading.Tasks;
using System.Windows.Forms;

namespace RBX_Alt_Manager.Classes
{
    /// <summary>
    /// Thin adapter over CoreWebView2 exposing the small slice of the old Puppeteer
    /// Page API that call sites in AccountManager.cs still use.
    /// </summary>
    internal class BrowserPage
    {
        private readonly CoreWebView2 Core;

        public BrowserPage(CoreWebView2 core) { Core = core; }

        public async Task EvaluateExpressionAsync(string script)
        {
            try { await Core.ExecuteScriptAsync(script); } catch { }
        }

        public async Task<string> EvaluateStringAsync(string script)
        {
            try
            {
                string raw = await Core.ExecuteScriptAsync(script);
                if (string.IsNullOrEmpty(raw) || raw == "null") return null;
                return System.Text.Json.JsonSerializer.Deserialize<string>(raw);
            }
            catch { return null; }
        }

        public async Task<BrowserElement> WaitForSelectorAsync(string selector, WaitForSelectorOptions options = null)
        {
            int timeout = options?.Timeout ?? 30000;
            var start = DateTime.Now;

            while ((DateTime.Now - start).TotalMilliseconds < timeout)
            {
                string found = await EvaluateStringAsync($"(function(){{ return document.querySelector({Quote(selector)}) ? 'y' : null; }})()");
                if (found == "y") return new BrowserElement(Core, selector);
                await Task.Delay(200);
            }

            return null;
        }

        public async Task TypeAsync(string selector, string text)
        {
            var rng = new Random();

            await EvaluateExpressionAsync($"(function(){{ var e=document.querySelector({Quote(selector)}); if(e){{ e.focus(); var s=Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value').set; s.call(e,''); e.dispatchEvent(new Event('input',{{bubbles:true}})); }} }})()");

            var sb = new System.Text.StringBuilder();
            foreach (char ch in text ?? string.Empty)
            {
                sb.Append(ch);
                string current = sb.ToString();

                await EvaluateExpressionAsync($"(function(){{ var e=document.querySelector({Quote(selector)}); if(e){{ var s=Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value').set; s.call(e,{Quote(current)}); e.dispatchEvent(new Event('input',{{bubbles:true}})); e.dispatchEvent(new KeyboardEvent('keyup',{{bubbles:true}})); }} }})()");

                await Task.Delay(rng.Next(60, 170));
            }

            await EvaluateExpressionAsync($"(function(){{ var e=document.querySelector({Quote(selector)}); if(e) e.dispatchEvent(new Event('change',{{bubbles:true}})); }})()");
        }

        public async Task ClickAsync(string selector)
            => await EvaluateExpressionAsync($"(function(){{ var e=document.querySelector({Quote(selector)}); if(e) e.click(); }})()");

        public async Task<string> GetSecurityCookieAsync()
        {
            try
            {
                var cookies = await Core.CookieManager.GetCookiesAsync("https://www.roblox.com");
                return cookies.FirstOrDefault(c => c.Name == ".ROBLOSECURITY")?.Value;
            }
            catch { return null; }
        }

        internal static string Quote(string value)
            => System.Text.Json.JsonSerializer.Serialize(value ?? string.Empty);
    }

    internal class BrowserElement
    {
        private readonly CoreWebView2 Core;
        private readonly string Selector;

        public BrowserElement(CoreWebView2 core, string selector) { Core = core; Selector = selector; }

        public async Task ClickAsync()
        {
            try { await Core.ExecuteScriptAsync($"(function(){{ var e=document.querySelector({BrowserPage.Quote(Selector)}); if(e) e.click(); }})()"); } catch { }
        }
    }

    internal class WaitForSelectorOptions
    {
        public int Timeout { get; set; } = 30000;
    }

    /// <summary>
    /// WebView2-backed replacement for the old Puppeteer AccountBrowser.
    /// Opens a real Edge Chromium window per account; preserves the public surface
    /// AccountManager.cs relies on.
    /// </summary>
    internal class AccountBrowser
    {
        private static Dictionary<int, Vector2> ScreenGrid;

        public Vector2 Size = new Vector2(880, 740), Position;
        public int Index = -1;

        private bool CaptureLogin;
        private string CapturedPassword = string.Empty;

        public AccountBrowser() { }

        public AccountBrowser(Account account, string Url = null, string Script = null, Func<BrowserPage, Task> PostNavigation = null)
        {
            _ = LaunchBrowser(Url ?? string.Empty, Script: Script, PostNavigation: PostNavigation, Cookie: account.SecurityToken, ProfileKey: account.UserID.ToString());
        }

        public async Task LaunchBrowser(string Url = "https://www.roblox.com/", Func<Task> PostPageCreation = null, Func<BrowserPage, Task> PostNavigation = null, string Script = null, string[] Arguments = null, string Cookie = null, string ProfileKey = null)
        {
            if (string.IsNullOrEmpty(Url)) Url = "https://www.roblox.com/";

            ComputePosition();

            string extraArgs = BuildBrowserArguments(Arguments);
            string userDataFolder = ProfileFolderFor(ProfileKey);
            string userAgent = DeriveUserAgent(ProfileKey);

            var host = new WebView2Host((int)Size.X, (int)Size.Y, (int)Position.X, (int)Position.Y, extraArgs, userDataFolder, userAgent);
            await host.InitializeAsync();

            var core = host.Core;
            var page = new BrowserPage(core);

            if (!string.IsNullOrEmpty(Cookie))
            {
                var c = core.CookieManager.CreateCookie(".ROBLOSECURITY", Cookie, ".roblox.com", "/");
                c.IsHttpOnly = true;
                c.IsSecure = true;
                c.Expires = DateTime.Now.AddYears(1);
                core.CookieManager.AddOrUpdateCookie(c);
            }

            if (PostPageCreation != null) try { await PostPageCreation(); } catch { }

            if (CaptureLogin)
            {
                try { core.CookieManager.DeleteAllCookies(); } catch { }

                bool busy = false, added = false;
                async Task TryCapture()
                {
                    if (busy || added) return;

                    string current = core.Source;
                    if (string.IsNullOrEmpty(current) || !current.Contains("/home")) return;

                    busy = true;
                    try
                    {
                        var cookies = await core.CookieManager.GetCookiesAsync("https://www.roblox.com");
                        var rsec = cookies.FirstOrDefault(x => x.Name == ".ROBLOSECURITY");

                        if (rsec != null)
                        {
                            string token = rsec.Value, password = CapturedPassword;
                            Account result = await Task.Run(() => AccountManager.AddAccount(token, password));

                            if (result != null)
                            {
                                added = true;
                                host.InvokeIfRequired(() => host.Close());
                            }
                            else
                            {
                                Program.Logger.Error("[AccountBrowser] captured cookie but account validation failed");
                                host.InvokeIfRequired(() => MessageBox.Show(host, "No se pudo agregar la cuenta (Roblox no respondió a tiempo).\nRecarga la página (F5) para reintentar.", "Roblox Account Manager", MessageBoxButtons.OK, MessageBoxIcon.Warning));
                            }
                        }
                    }
                    catch (Exception ex) { Program.Logger.Error($"[AccountBrowser] cookie capture failed: {ex}"); }
                    finally { busy = false; }
                }

                core.SourceChanged += async (s, e) => await TryCapture();
                core.NavigationCompleted += async (s, e) => await TryCapture();
            }

            host.Show();

            if (CaptureLogin) _ = WatchTypedPasswordAsync(host);

            await host.NavigateAndWaitAsync(Url);

            if (!string.IsNullOrEmpty(Script)) await page.EvaluateExpressionAsync(Script);

            if (PostNavigation != null) try { await PostNavigation(page); } catch { }
        }

        private async Task WatchTypedPasswordAsync(WebView2Host host)
        {
            while (host != null && !host.IsDisposed)
            {
                try
                {
                    string raw = await host.Core.ExecuteScriptAsync("(document.getElementById('login-password')||document.getElementById('signup-password')||{}).value || null");
                    if (!string.IsNullOrEmpty(raw) && raw != "null")
                        CapturedPassword = System.Text.Json.JsonSerializer.Deserialize<string>(raw);
                }
                catch { }

                await Task.Delay(150);
            }
        }

        public async Task Login(string Username = "", string Password = "", string[] Arguments = null, string ProfileKey = null)
        {
            CleanupTempProfiles();
            // a shared profile let concurrent/bulk logins clobber each other's session cookie
            ProfileKey ??= "_login_" + Guid.NewGuid().ToString("N");
            CaptureLogin = true;
            CapturedPassword = Password ?? string.Empty;
            await LaunchBrowser("https://www.roblox.com/login", Arguments: Arguments, ProfileKey: ProfileKey,
                PostNavigation: async (page) => await LoginTask(page, Username, Password));
        }

        public async Task Signup()
        {
            CleanupTempProfiles();
            CaptureLogin = true;
            CapturedPassword = string.Empty;
            // fresh isolated profile + its own stable UA, same as a per-account login
            await LaunchBrowser("https://www.roblox.com/", ProfileKey: "_signup_" + Guid.NewGuid().ToString("N"),
                PostNavigation: async (page) => await page.EvaluateExpressionAsync(@"document.body.classList.remove(""light-theme"");document.body.classList.add(""dark-theme"");"));
        }

        public async Task LoginTask(BrowserPage page, string Username = "", string Password = "")
        {
            await page.EvaluateExpressionAsync(@"document.body.classList.remove(""light-theme"");document.body.classList.add(""dark-theme"");");

            try
            {
                if (!string.IsNullOrEmpty(Username) && await page.WaitForSelectorAsync("#login-username", new WaitForSelectorOptions { Timeout = 5000 }) != null)
                    await page.TypeAsync("#login-username", Username);
                if (!string.IsNullOrEmpty(Password) && await page.WaitForSelectorAsync("#login-password", new WaitForSelectorOptions { Timeout = 5000 }) != null)
                    await page.TypeAsync("#login-password", Password);

                if (!string.IsNullOrEmpty(Username) && !string.IsNullOrEmpty(Password))
                {
                    await Task.Delay(new Random().Next(400, 1100));
                    await page.ClickAsync("#login-button");
                }
            }
            catch (Exception ex) { Program.Logger.Error($"An exception was caught while trying to automatically log in: {ex}"); }
        }

        private static void CleanupTempProfiles()
        {
            try
            {
                var baseDir = new DirectoryInfo(Path.Combine(Environment.CurrentDirectory, "WebView2Data"));
                if (!baseDir.Exists) return;

                foreach (var dir in baseDir.GetDirectories())
                    if ((dir.Name == "_login" || dir.Name.StartsWith("_login_") || dir.Name.StartsWith("_signup_")) && dir.LastWriteTime < DateTime.Now.AddHours(-1))
                        try { dir.Delete(true); } catch { } // still locked by an open window
            }
            catch { }
        }

        private static string ProfileFolderFor(string key)
        {
            string baseDir = Path.Combine(Environment.CurrentDirectory, "WebView2Data");

            if (string.IsNullOrWhiteSpace(key))
                return Path.Combine(baseDir, "_default");

            // isolate each account's cookies/cache/storage so Roblox sees separate browsers
            string safe = new string(key.Where(c => char.IsLetterOrDigit(c) || c == '_' || c == '-').ToArray());
            return Path.Combine(baseDir, string.IsNullOrEmpty(safe) ? "_default" : safe);
        }

        // Realistic, recent Chrome/Edge UAs. A stable key maps to a stable UA so an
        // account's fingerprint stays consistent across logins (a shifting UA is itself a flag).
        private static readonly string[] UserAgentPool =
        {
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36 Edg/123.0.0.0",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36 Edg/125.0.0.0",
        };

        private static string DeriveUserAgent(string key)
        {
            if (string.IsNullOrWhiteSpace(key)) return null; // use WebView2's real default for the login window

            int hash = 0;
            foreach (char c in key) hash = unchecked(hash * 31 + c);
            return UserAgentPool[Math.Abs(hash) % UserAgentPool.Length];
        }

        private void ComputePosition()
        {
            if (Index >= 0 && ScreenGrid != null && ScreenGrid.Count > 0)
                Position = ScreenGrid[Index % (ScreenGrid.Count - 1)];
            else
                Position = new Vector2(Screen.PrimaryScreen.WorkingArea.Width / 2 - (Size.X / 2), Screen.PrimaryScreen.WorkingArea.Height / 2 - (Size.Y / 2));
        }

        private static string BuildBrowserArguments(string[] Arguments)
        {
            var args = new List<string>(Arguments ?? Array.Empty<string>());

            string ConfigPath = Path.Combine(Environment.CurrentDirectory, "BrowserConfig.json");
            if (File.Exists(ConfigPath) && File.ReadAllText(ConfigPath).TryParseJson(out BrowserConfig Config) && Config?.CustomArguments != null)
                args.AddRange(Config.CustomArguments);

            string ProxiesPath = Path.Combine(Environment.CurrentDirectory, "proxies.txt");
            if (AccountManager.General.Get<bool>("UseProxies") && File.Exists(ProxiesPath))
            {
                var lines = File.ReadAllLines(ProxiesPath).Where(l => !string.IsNullOrWhiteSpace(l)).ToList();
                if (lines.Count > 0)
                {
                    string proxy = lines[new Random().Next(lines.Count)];
                    int at = proxy.IndexOf('@');
                    if (at >= 0) proxy = proxy.Substring(0, at); // WebView2 cannot take proxy auth inline
                    args.Add($"--proxy-server={proxy}");
                }
            }

            return string.Join(" ", args);
        }

        public static void CreateGrid(Vector2 Size)
        {
            ScreenGrid = new Dictionary<int, Vector2>();

            for (int x = 0; x < SystemInformation.VirtualScreen.Width; x += (int)Size.X)
                for (int y = 0; y < SystemInformation.VirtualScreen.Height - (Size.Y / 2); y += (int)Size.Y)
                    ScreenGrid.Add(ScreenGrid.Count, new Vector2(x, y));
        }
    }

    /// <summary>
    /// Shared single-window browser used for interactive login and "browser mode".
    /// Keeps the CefBrowser.Instance call surface AccountManager.cs expects.
    /// </summary>
    internal class CefBrowser
    {
        private static CefBrowser _Instance;
        public static CefBrowser Instance => _Instance ??= new CefBrowser();

        private WebView2Host Host;
        private bool BrowserMode;
        private string Password = string.Empty;

        private async Task EnsureHostAsync()
        {
            if (Host == null || Host.IsDisposed)
            {
                Host = new WebView2Host(880, 740, -1, -1, string.Empty);
                await Host.InitializeAsync();
                Host.Core.SourceChanged += Core_SourceChanged;
            }
        }

        public async void Login()
        {
            await EnsureHostAsync();
            BrowserMode = false;
            Host.Core.CookieManager.DeleteAllCookies();
            Host.Show();
            Host.Core.Navigate("https://www.roblox.com/login");
            _ = WatchPasswordAsync();
        }

        public async void EnterBrowserMode(Account account, string URL = null)
        {
            await EnsureHostAsync();
            BrowserMode = true;

            var c = Host.Core.CookieManager.CreateCookie(".ROBLOSECURITY", account.SecurityToken, ".roblox.com", "/");
            c.IsHttpOnly = true;
            c.IsSecure = true;
            c.Expires = DateTime.Now.AddYears(1);
            Host.Core.CookieManager.AddOrUpdateCookie(c);

            Host.Show();
            Host.Core.Navigate(URL ?? "https://www.roblox.com/home");
        }

        private async Task WatchPasswordAsync()
        {
            while (Host != null && !Host.IsDisposed && Host.Visible)
            {
                try
                {
                    string raw = await Host.Core.ExecuteScriptAsync("(document.getElementById('login-password')||document.getElementById('signup-password')||{}).value || null");
                    if (!string.IsNullOrEmpty(raw) && raw != "null")
                        Password = System.Text.Json.JsonSerializer.Deserialize<string>(raw);
                }
                catch { }

                await Task.Delay(100);
            }
        }

        private async void Core_SourceChanged(object sender, CoreWebView2SourceChangedEventArgs e)
        {
            string url = Host.Core.Source;
            Program.Logger.Info($"Browser Navigated to {url}");

            if (url.Contains("/home") && !BrowserMode)
            {
                var cookies = await Host.Core.CookieManager.GetCookiesAsync("https://www.roblox.com");
                var rsec = cookies.FirstOrDefault(x => x.Name == ".ROBLOSECURITY");

                if (rsec != null)
                {
                    AccountManager.AddAccount(rsec.Value, Password);
                    Host.Core.CookieManager.DeleteAllCookies();
                    Password = string.Empty;
                    Host.InvokeIfRequired(() => Host.Hide());
                }
            }
        }
    }

    internal class BrowserConfig
    {
        public Dictionary<string, string> PreNavigateActions;

        public List<string> PreNavigateScripts;
        public List<string> PostNavigateScripts;

        public List<string> CustomArguments;
    }
}
