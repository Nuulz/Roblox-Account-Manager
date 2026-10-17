using Microsoft.Web.WebView2.Core;
using Microsoft.Web.WebView2.WinForms;
using System;
using System.IO;
using System.Threading.Tasks;
using System.Windows.Forms;

namespace RBX_Alt_Manager.Classes
{
    internal class WebView2Host : Form
    {
        public WebView2 View { get; private set; }
        public CoreWebView2 Core => View?.CoreWebView2;

        private readonly string ExtraArgs;
        private TaskCompletionSource<bool> NavCompletion;

        public WebView2Host(int width, int height, int x, int y, string extraArgs)
        {
            ExtraArgs = extraArgs ?? string.Empty;

            Text = "Roblox Account Manager";
            StartPosition = x >= 0 && y >= 0 ? FormStartPosition.Manual : FormStartPosition.CenterScreen;
            ClientSize = new System.Drawing.Size(width, height);
            if (x >= 0 && y >= 0) Location = new System.Drawing.Point(x, y);

            View = new WebView2 { Dock = DockStyle.Fill };
            Controls.Add(View);
        }

        public async Task InitializeAsync()
        {
            string userData = Path.Combine(Environment.CurrentDirectory, "WebView2Data");

            CoreWebView2Environment env;
            try
            {
                var options = new CoreWebView2EnvironmentOptions(string.IsNullOrWhiteSpace(ExtraArgs) ? null : ExtraArgs);
                env = await CoreWebView2Environment.CreateAsync(null, userData, options);
            }
            catch (Exception ex)
            {
                throw new WebView2RuntimeMissingException(ex);
            }

            await View.EnsureCoreWebView2Async(env);

            View.CoreWebView2.NavigationCompleted += (s, e) =>
            {
                var tcs = NavCompletion;
                if (tcs != null && !tcs.Task.IsCompleted) tcs.TrySetResult(e.IsSuccess);
            };
        }

        public async Task NavigateAndWaitAsync(string url)
        {
            NavCompletion = new TaskCompletionSource<bool>();
            Core.Navigate(url);

            var completed = await Task.WhenAny(NavCompletion.Task, Task.Delay(300000));
            if (completed != NavCompletion.Task) NavCompletion.TrySetResult(false);
        }
    }

    internal class WebView2RuntimeMissingException : Exception
    {
        public WebView2RuntimeMissingException(Exception inner)
            : base("The Microsoft Edge WebView2 Runtime is required. Download it at https://developer.microsoft.com/microsoft-edge/webview2/", inner) { }
    }
}
