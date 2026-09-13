using System.Windows;
namespace Tinrec;
public partial class App : System.Windows.Application
{
    private Mutex? instance;
    protected override void OnStartup(StartupEventArgs e)
    {
        instance = new Mutex(true, "Local\\Tinrec.Desktop", out bool first);
        if (!first) { System.Windows.MessageBox.Show("O Tinrec já está aberto."); Shutdown(); return; }
        base.OnStartup(e);
    }
    protected override void OnExit(ExitEventArgs e) { instance?.Dispose(); base.OnExit(e); }
}
