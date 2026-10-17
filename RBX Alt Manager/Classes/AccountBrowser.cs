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
            string js = $@"(function(){{ var e=document.querySelector({Quote(selector)}); if(e){{ var s=Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value').set; s.call(e,{Quote(text)}); e.dispatchEvent(new Event('input',{{bubbles:true}})); e.dispatchEvent(new Event('change',{{bubbles:true}})); }} }})()";
            await EvaluateExpressionAsync(js);
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

        public AccountBrowser() { }

        public AccountBrowser(Account account, string Url = null, string Script = null, Func<BrowserPage, Task> PostNavigation = null)
        {
            _ = LaunchBrowser(Url ?? string.Empty, Script: Script, PostNavigation: PostNavigation, Cookie: account.SecurityToken);
        }

        public async Task LaunchBrowser(string Url = "https://www.roblox.com/", Func<Task> PostPageCreation = null, Func<BrowserPage, Task> PostNavigation = null, string Script = null, string[] Arguments = null, string Cookie = null)
        {
            if (string.IsNullOrEmpty(Url)) Url = "https://www.roblox.com/";

            ComputePosition();

            string extraArgs = BuildBrowserArguments(Arguments);

            var host = new WebView2Host((int)Size.X, (int)Size.Y, (int)Position.X, (int)Position.Y, extraArgs);
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

            host.Show();
            await host.NavigateAndWaitAsync(Url);

            if (!string.IsNullOrEmpty(Script)) await page.EvaluateExpressionAsync(Script);

            if (PostNavigation != null) try { await PostNavigation(page); } catch { }
        }

        public async Task Login(string Username = "", string Password = "", string[] Arguments = null)
            => await LaunchBrowser("https://www.roblox.com/login", Arguments: Arguments, PostNavigation: async (page) => await LoginTask(page, Username, Password));

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
                    await page.ClickAsync("#login-button");
            }
            catch (Exception ex) { Program.Logger.Error($"An exception was caught while trying to automatically log in: {ex}"); }
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
