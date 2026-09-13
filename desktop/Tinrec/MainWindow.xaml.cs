using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Threading;
using ScreenRecorderLib;
namespace Tinrec;
public partial class MainWindow : Window
{
    private readonly LocalStore store;
    private readonly ApiClient api;
    private readonly UploadWorker worker;
    private readonly CancellationTokenSource lifetime = new();
    private readonly DispatcherTimer ticker = new() { Interval = TimeSpan.FromSeconds(1) };
    private CaptureService? capture;
    private UploadItem? current;
    private ScreenRect? region;
    private bool recording, busy, connecting;
    private int ticks;
    private sealed record DisplayChoice(string Label, System.Windows.Forms.Screen Screen);
    private sealed record AudioChoice(string Label, string? Id);
    public MainWindow() : this(null) { }
    public MainWindow(LocalStore? localStore)
    {
        InitializeComponent();
        try { store = localStore ?? new LocalStore(); } catch (Exception e) { System.Windows.MessageBox.Show("Não foi possível ler os dados locais. Os arquivos foram preservados.\n" + e.Message, "Tinrec"); throw; }
        api = new ApiClient(store); worker = new UploadWorker(store, api);
        worker.Changed += () => Dispatcher.BeginInvoke(() => { RefreshQueue(); if (current is { Status: not "recording", LinkCopied: false } item && item.Url.Length > 0) { TryCopy(item); } });
        var p = store.Preferences; SiteUrl.Text = p.SiteUrl; Prefix.Text = p.Prefix; Resolution.SelectedIndex = p.Resolution == 720 ? 0 : 1; Microphone.IsChecked = p.Microphone; SystemAudio.IsChecked = p.SystemAudio;
        ConnectionStatus.Text = string.IsNullOrEmpty(p.Token) ? "Conecte sua conta para enviar os vídeos." : "Aplicativo conectado. Seus envios serão retomados automaticamente.";
        SettingsExpander.IsExpanded = string.IsNullOrEmpty(p.SiteUrl);
        ResetTitle(); RefreshQueue();
        Loaded += (_, _) =>
        {
            try
            {
                Recorder.SetExcludeFromCapture(new WindowInteropHelper(this).Handle, true);
                Displays.ItemsSource = System.Windows.Forms.Screen.AllScreens.Select((s, i) => new DisplayChoice($"Monitor {i + 1} · {s.Bounds.Width} × {s.Bounds.Height}", s)).ToList(); Displays.SelectedIndex = 0;
                Inputs.ItemsSource = new[] { new AudioChoice("Dispositivo padrão", null) }.Concat(Recorder.GetSystemAudioCaptureDevices().Select(d => new AudioChoice(d.FriendlyName, d.DeviceName))).ToList();
                Outputs.ItemsSource = new[] { new AudioChoice("Dispositivo padrão", null) }.Concat(Recorder.GetSystemAudioLoopbackDevices().Select(d => new AudioChoice(d.FriendlyName, d.DeviceName))).ToList();
                Inputs.SelectedItem = Inputs.Items.Cast<AudioChoice>().FirstOrDefault(x => x.Id == p.InputDevice) ?? Inputs.Items[0]; Outputs.SelectedItem = Outputs.Items.Cast<AudioChoice>().FirstOrDefault(x => x.Id == p.OutputDevice) ?? Outputs.Items[0];
            }
            catch (Exception e) { Message.Text = "Não foi possível inicializar a captura. Verifique o Visual C++ Redistributable e o Media Feature Pack. " + e.Message; StartButton.IsEnabled = false; }
        };
        ticker.Tick += async (_, _) => { if (capture is not null) { TimerLabel.Text = TimeSpan.FromMilliseconds(capture.ElapsedMs).ToString(@"hh\:mm\:ss"); if (recording && !busy) { PauseButton.Content = capture.Paused ? "Retomar" : "Pausar"; StateLabel.Text = capture.Paused ? "Pausado" : "● Gravando"; } } if (++ticks % 20 == 0) { if (recording && !busy && new DriveInfo(Path.GetPathRoot(store.Root)!).AvailableFreeSpace < 100 * 1024 * 1024) { Stop_Click(this, new RoutedEventArgs()); Message.Text = "A gravação foi encerrada porque o computador está com pouco espaço."; } if (!string.IsNullOrEmpty(store.Preferences.Token)) await RunWorker(); } }; ticker.Start();
        Closing += OnClosing;
    }
    private void RefreshQueue() { var selected = (Queue.SelectedItem as UploadItem)?.Id; Queue.ItemsSource = null; Queue.ItemsSource = store.Items.AsEnumerable().Reverse().ToList(); Queue.SelectedItem = store.Items.FirstOrDefault(x => x.Id == selected); }
    private void ResetTitle() => VideoTitle.Text = $"{store.Preferences.Prefix} — {DateTime.Now:dd-MM-yyyy HH-mm-ss}";
    private void SavePreferences()
    {
        var p = store.Preferences; var next = string.IsNullOrWhiteSpace(SiteUrl.Text) ? "" : ApiClient.ValidateUrl(SiteUrl.Text);
        if (next != p.SiteUrl) { if (!string.IsNullOrEmpty(p.SiteUrl) && store.Items.Any(x => x.Status is "queued" or "uploading")) throw new Exception("Conclua os envios pendentes antes de mudar o endereço do site."); p.Token = ""; }
        p.SiteUrl = next; p.Prefix = string.IsNullOrWhiteSpace(Prefix.Text) ? "Atendimento" : Prefix.Text.Trim()[..Math.Min(Prefix.Text.Trim().Length, 120)]; p.Resolution = Resolution.SelectedIndex == 0 ? 720 : 1080; p.Microphone = Microphone.IsChecked == true; p.SystemAudio = SystemAudio.IsChecked == true; p.InputDevice = (Inputs.SelectedItem as AudioChoice)?.Id; p.OutputDevice = (Outputs.SelectedItem as AudioChoice)?.Id; store.Save();
    }
    private void Save_Click(object sender, RoutedEventArgs e) { try { SavePreferences(); ResetTitle(); Message.Text = "Preferências salvas."; } catch (Exception ex) { Message.Text = ex.Message; } }
    private async void Connect_Click(object sender, RoutedEventArgs e)
    {
        if (connecting) return; connecting = true; ConnectButton.IsEnabled = false;
        try
        {
            SavePreferences(); var response = await api.Request("/devices/start", new { }, false, lifetime.Token);
            var code = response.GetProperty("code").GetString()!; var secret = response.GetProperty("secret").GetString()!;
            ConnectionStatus.Text = "Confirme este código no site: " + code;
            Open(response.GetProperty("url").GetString()!);
            using var deadline = CancellationTokenSource.CreateLinkedTokenSource(lifetime.Token); deadline.CancelAfter(TimeSpan.FromMinutes(10));
            while (true) { await Task.Delay(3000, deadline.Token); var poll = await api.Request("/devices/poll", new { code, secret }, false, deadline.Token); if (poll.TryGetProperty("token", out var t)) { store.Preferences.Token = t.GetString()!; store.Save(); ConnectionStatus.Text = "Conta conectada. Pronto para enviar."; await RunWorker(); break; } }
        }
        catch (OperationCanceledException) { ConnectionStatus.Text = "Conexão cancelada ou expirada. Tente novamente."; }
        catch (Exception ex) { ConnectionStatus.Text = ex.Message; }
        finally { connecting = false; ConnectButton.IsEnabled = true; }
    }
    private void Display_Changed(object sender, SelectionChangedEventArgs e) { region = null; if (RegionLabel is not null) RegionLabel.Text = "Tela inteira"; }
    private void FullScreen_Click(object sender, RoutedEventArgs e) { region = null; RegionLabel.Text = "Tela inteira"; }
    private void Area_Click(object sender, RoutedEventArgs e) { if (Displays.SelectedItem is not DisplayChoice d) return; var selector = new AreaSelector(d.Screen); if (selector.ShowDialog() == true) { region = selector.Region; RegionLabel.Text = $"{region!.Width} × {region.Height}"; } }
    private async void Start_Click(object sender, RoutedEventArgs e)
    {
        if (busy || recording) return; busy = true; SetControls(false);
        try
        {
            SavePreferences(); if (Displays.SelectedItem is not DisplayChoice display) throw new Exception("Escolha um monitor.");
            var title = VideoTitle.Text.Trim(); if (title.Length == 0) throw new Exception("Dê um título para esta gravação.");
            var disk = new DriveInfo(Path.GetPathRoot(store.Root)!); if (disk.AvailableFreeSpace < 200 * 1024 * 1024) throw new Exception("Há pouco espaço no computador. Libere ao menos 200 MB para gravar.");
            current = new UploadItem { Title = title, Resolution = store.Preferences.Resolution }; current.FilePath = Path.Combine(store.Root, "Recordings", current.Id + ".mp4"); store.Items.Add(current); store.Save();
            if (!string.IsNullOrEmpty(store.Preferences.Token))
            {
                StateLabel.Text = "Preparando o link…";
                try { using var reserveTimeout = new CancellationTokenSource(TimeSpan.FromSeconds(5)); await api.Reserve(current, reserveTimeout.Token); } catch { Message.Text = "A gravação ficará salva localmente. O link será gerado quando a conexão estiver disponível."; }
            }
            for (int i = 3; i > 0; i--) { StateLabel.Text = $"Começando em {i}…"; await Task.Delay(1000, lifetime.Token); }
            capture = new CaptureService(); capture.Failed += error => Dispatcher.BeginInvoke(() => OnCaptureFailed(error));
            await capture.Start(current.FilePath, display.Screen.DeviceName, region, store.Preferences);
            recording = true; StateLabel.Text = "● Gravando"; PauseButton.IsEnabled = true; StopButton.IsEnabled = true; RefreshQueue();
        }
        catch (Exception ex) { Message.Text = "Não foi possível iniciar: " + ex.Message; if (current is not null) { current.Status = "error"; current.Error = ex.Message; store.Save(); } capture?.Dispose(); capture = null; SetControls(true); RefreshQueue(); }
        finally { busy = false; }
    }
    private void OnCaptureFailed(string error) { if (current is not null) { current.Status = "error"; current.Error = error; store.Save(); } recording = false; StateLabel.Text = "Gravação interrompida"; Message.Text = "O arquivo local foi preservado. " + error; capture?.Dispose(); capture = null; SetControls(true); RefreshQueue(); }
    private void Pause_Click(object sender, RoutedEventArgs e) { capture?.TogglePause(); PauseButton.Content = capture?.Paused == true ? "Retomar" : "Pausar"; StateLabel.Text = capture?.Paused == true ? "Pausado" : "● Gravando"; }
    private async void Stop_Click(object sender, RoutedEventArgs e)
    {
        if (busy || !recording || capture is null || current is null) return; busy = true; StopButton.IsEnabled = false; PauseButton.IsEnabled = false; StateLabel.Text = "Finalizando…";
        try
        {
            var item = current; await capture.Stop(); item.DurationMs = capture.ElapsedMs; item.Status = "queued"; store.Save(); recording = false; StateLabel.Text = "Gravação salva";
            if (item.Url.Length > 0) { TryCopy(item); } else Message.Text = "Vídeo salvo no computador. Conecte sua conta para enviar e gerar o link.";
            ResetTitle(); RefreshQueue(); _ = RunWorker();
        }
        catch (Exception ex) { OnCaptureFailed(ex.Message); }
        finally { capture?.Dispose(); capture = null; busy = false; recording = false; SetControls(true); }
    }
    private void SetControls(bool enabled) { CaptureSettings.IsEnabled = enabled; SettingsPanel.IsEnabled = enabled; StartButton.IsEnabled = enabled; if (enabled) { PauseButton.IsEnabled = false; StopButton.IsEnabled = false; PauseButton.Content = "Pausar"; } }
    private async Task RunWorker() { try { await worker.Run(lifetime.Token); } catch (OperationCanceledException) { } catch (Exception ex) { Message.Text = ex.Message; } }
    private async void Retry_Click(object sender, RoutedEventArgs e) { if (worker.Busy) return; foreach (var item in store.Items.Where(x => x.Status == "queued")) { item.NextAttemptAt = null; } await RunWorker(); }
    private void TryCopy(UploadItem item) { try { System.Windows.Clipboard.SetText(item.Url); item.LinkCopied = true; store.Save(); Message.Text = "Link copiado! O vídeo estará disponível após o envio e o processamento do Drive."; } catch { Message.Text = "O link está pronto. Use Copiar link quando a área de transferência estiver disponível."; } }
    private void Copy_Click(object sender, RoutedEventArgs e) { var item = Queue.SelectedItem as UploadItem ?? store.Items.LastOrDefault(); if (item?.Url.Length > 0) { TryCopy(item); } else Message.Text = "O link aparece após a conexão com o site. O arquivo local permanece salvo."; }
    private static void Open(string url) => Process.Start(new ProcessStartInfo(url) { UseShellExecute = true });
    private void Library_Click(object sender, RoutedEventArgs e) { try { Open(ApiClient.ValidateUrl(store.Preferences.SiteUrl)); } catch (Exception ex) { Message.Text = ex.Message; } }
    private void Files_Click(object sender, RoutedEventArgs e) => Open(Path.Combine(store.Root, "Recordings"));
    private void OnClosing(object? sender, CancelEventArgs e)
    {
        if (recording || busy) { e.Cancel = true; Message.Text = "Pare a gravação antes de fechar o aplicativo."; return; }
        if (worker.Busy && System.Windows.MessageBox.Show("O envio será pausado e retomado ao abrir o Tinrec. Fechar agora?", "Tinrec", MessageBoxButton.YesNo) != MessageBoxResult.Yes) { e.Cancel = true; return; }
        ticker.Stop(); lifetime.Cancel(); store.Save(); capture?.Dispose();
    }
}
