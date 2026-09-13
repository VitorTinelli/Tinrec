using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;
using ScreenRecorderLib;
using WPoint = System.Windows.Point;
using Brushes = System.Windows.Media.Brushes;
namespace Tinrec;
public sealed class AreaSelector : Window
{
    private readonly Canvas canvas = new();
    private readonly System.Windows.Shapes.Rectangle selection = new() { Stroke = Brushes.White, StrokeThickness = 2, Fill = new SolidColorBrush(System.Windows.Media.Color.FromArgb(30, 255, 255, 255)) };
    private POINT? start;
    private readonly System.Drawing.Rectangle bounds;
    public ScreenRect? Region { get; private set; }
    [StructLayout(LayoutKind.Sequential)] private struct POINT { public int X, Y; }
    [DllImport("user32.dll")] private static extern bool GetCursorPos(out POINT point);
    [DllImport("user32.dll")] private static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);
    public AreaSelector(System.Windows.Forms.Screen screen)
    {
        bounds = screen.Bounds; WindowStyle = WindowStyle.None; AllowsTransparency = true; Background = new SolidColorBrush(System.Windows.Media.Color.FromArgb(110, 20, 15, 35)); Topmost = true; ShowInTaskbar = false; Cursor = System.Windows.Input.Cursors.Cross; Content = canvas;
        canvas.Children.Add(new TextBlock { Text = "Arraste para selecionar uma área · Esc para cancelar", Foreground = Brushes.White, FontSize = 18, Margin = new Thickness(25) }); canvas.Children.Add(selection);
        SourceInitialized += (_, _) => { var hwnd = new WindowInteropHelper(this).Handle; SetWindowPos(hwnd, new IntPtr(-1), bounds.X, bounds.Y, bounds.Width, bounds.Height, 0x0040); Recorder.SetExcludeFromCapture(hwnd, true); };
        MouseLeftButtonDown += (_, e) => { GetCursorPos(out var p); start = p; CaptureMouse(); };
        MouseMove += (_, _) => { if (start is not { } s) return; GetCursorPos(out var p); var a = PointFromScreen(new WPoint(Math.Min(s.X, p.X), Math.Min(s.Y, p.Y))); var b = PointFromScreen(new WPoint(Math.Max(s.X, p.X), Math.Max(s.Y, p.Y))); Canvas.SetLeft(selection, a.X); Canvas.SetTop(selection, a.Y); selection.Width = b.X - a.X; selection.Height = b.Y - a.Y; };
        MouseLeftButtonUp += (_, _) => { if (start is not { } s) return; GetCursorPos(out var p); ReleaseMouseCapture(); int left = Math.Clamp(Math.Min(s.X, p.X) - bounds.X, 0, bounds.Width - 2), top = Math.Clamp(Math.Min(s.Y, p.Y) - bounds.Y, 0, bounds.Height - 2); int width = Math.Min(Math.Abs(p.X - s.X), bounds.Width - left) / 2 * 2, height = Math.Min(Math.Abs(p.Y - s.Y), bounds.Height - top) / 2 * 2; if (width < 16 || height < 16) { start = null; return; } Region = new ScreenRect(left, top, width, height); DialogResult = true; };
        KeyDown += (_, e) => { if (e.Key == Key.Escape) DialogResult = false; };
    }
}
