using System;
using System.Drawing;

namespace RBX_Alt_Manager.Classes
{
    public class AccountRenderer : BrightIdeasSoftware.BaseRenderer
    {
        public override void Render(Graphics g, Rectangle r)
        {
            base.Render(g, r);

            Account account = RowObject as Account;
            TimeSpan diff = DateTime.Now - account.LastUse;
            bool isOld = diff.TotalDays > 20;

            if (isOld)
            {
                diff -= TimeSpan.FromDays(20);

                using (Brush b = new SolidBrush(Color.FromArgb(255, 255, 204, 77).Lerp(Color.FromArgb(255, 250, 26, 13), (float)Utilities.MapValue(diff.TotalSeconds, 0, 864000, 0, 1).Clamp(0, 1))))
                    g.FillEllipse(b, new Rectangle((int)(r.X + 3f * Program.Scale), (int)(r.Y + 2 * Program.Scale), (int)(4f * Program.Scale), (int)(4f * Program.Scale)));
            }

            if (AccountManager.General.Get<bool>("ShowPresence") && account.Presence != null && account.Presence.userPresenceType != UserPresenceType.Offline)
                using (Brush b = new SolidBrush(Presence.Colors[account.Presence.userPresenceType]))
                    g.FillEllipse(b, new Rectangle((int)(r.X + 3f * Program.Scale + (isOld ? (int)(6f * Program.Scale) : 0)), (int)(r.Y + 2 * Program.Scale), (int)(4f * Program.Scale), (int)(4f * Program.Scale)));

            float rightOffset = r.Right - 4;

            if (account.NeedsLogin)
            {
                const string Label = "Requiere login";

                using (Font f = new Font(ListView.Font.FontFamily, ListView.Font.Size * 0.85f, FontStyle.Bold))
                using (Brush b = new SolidBrush(Color.FromArgb(255, 235, 87, 87)))
                {
                    SizeF s = g.MeasureString(Label, f);
                    rightOffset -= s.Width;
                    g.DrawString(Label, f, b, rightOffset, r.Y + (r.Height - s.Height) / 2);
                    rightOffset -= 6;
                }
            }

            if (account.AntiAfkEnabled)
            {
                const string Label = "Anti-AFK";

                using (Font f = new Font(ListView.Font.FontFamily, ListView.Font.Size * 0.85f, FontStyle.Bold))
                using (Brush b = new SolidBrush(Color.FromArgb(255, 80, 200, 120)))
                {
                    SizeF s = g.MeasureString(Label, f);
                    rightOffset -= s.Width;
                    g.DrawString(Label, f, b, rightOffset, r.Y + (r.Height - s.Height) / 2);
                    rightOffset -= 6;
                }
            }

            if (account.GameSessionStart.HasValue)
            {
                TimeSpan elapsed = DateTime.Now - account.GameSessionStart.Value;
                string Label = elapsed.TotalHours >= 1
                    ? $"{(int)elapsed.TotalHours}h {elapsed.Minutes:D2}m"
                    : $"{elapsed.Minutes}m";

                using (Font f = new Font(ListView.Font.FontFamily, ListView.Font.Size * 0.85f, FontStyle.Bold))
                using (Brush b = new SolidBrush(Color.FromArgb(255, 100, 180, 255)))
                {
                    SizeF s = g.MeasureString(Label, f);
                    rightOffset -= s.Width;
                    g.DrawString(Label, f, b, rightOffset, r.Y + (r.Height - s.Height) / 2);
                }
            }

        }
    }
}