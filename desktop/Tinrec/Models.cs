using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
namespace Tinrec;
public sealed class Preferences
{
    public string SiteUrl { get; set; } = "";
    public string Token { get; set; } = "";
    public string Prefix { get; set; } = "Atendimento";
    public int Resolution { get; set; } = 1080;
    public bool Microphone { get; set; } = true;
    public bool SystemAudio { get; set; } = true;
    public string? InputDevice { get; set; }
    public string? OutputDevice { get; set; }
}
public sealed class UploadItem
{
    public string Id { get; set; } = Guid.NewGuid().ToString();
    public string Title { get; set; } = "";
    public string FilePath { get; set; } = "";
    public int Resolution { get; set; } = 1080;
    public long DurationMs { get; set; }
    public string Url { get; set; } = "";
    public string Status { get; set; } = "recording";
    public string? UploadUri { get; set; }
    public long UploadedBytes { get; set; }
    public DateTimeOffset? CompletedAt { get; set; }
    public string? Error { get; set; }
    public int Attempts { get; set; }
    public DateTimeOffset? NextAttemptAt { get; set; }
    public bool LinkCopied { get; set; }
    public string Display => $"{Title}  ·  {Status switch { "recording" => "Gravando", "queued" => "Na fila", "uploading" => "Enviando", "uploaded" => "Enviado", _ => "Precisa de atenção" }}{(Error is null ? "" : " — " + Error)}";
}
public sealed class LocalStore
{
    public string Root { get; } = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Tinrec");
    public Preferences Preferences { get; private set; } = new();
    public List<UploadItem> Items { get; private set; } = [];
    private readonly object gate = new();
    public LocalStore(string? root = null)
    {
        if (root is not null) Root = root;
        Directory.CreateDirectory(Root); Directory.CreateDirectory(Path.Combine(Root, "Recordings"));
        Preferences = Read<Preferences>("preferences.bin") ?? new(); Items = Read<List<UploadItem>>("queue.bin") ?? [];
        foreach (var item in Items.Where(x => x.Status == "recording")) { item.Status = "error"; item.Error = "Gravação interrompida. O arquivo local foi preservado para recuperação."; }
        Save();
    }
    private T? Read<T>(string file)
    {
        var path = Path.Combine(Root, file); if (!File.Exists(path)) return default;
        // Never replace an unreadable queue with an empty queue: preserve evidence for recovery.
        return JsonSerializer.Deserialize<T>(ProtectedData.Unprotect(File.ReadAllBytes(path), null, DataProtectionScope.CurrentUser));
    }
    public void Save()
    {
        lock (gate) { Write("preferences.bin", Preferences); Write("queue.bin", Items); }
    }
    private void Write<T>(string file, T value)
    {
        var path = Path.Combine(Root, file); var temp = path + ".tmp";
        var bytes = ProtectedData.Protect(JsonSerializer.SerializeToUtf8Bytes(value), null, DataProtectionScope.CurrentUser);
        using (var stream = new FileStream(temp, FileMode.Create, FileAccess.Write, FileShare.None)) { stream.Write(bytes); stream.Flush(true); }
        File.Move(temp, path, true);
    }
    public void Cleanup()
    {
        foreach (var item in Items.Where(x => x.Status == "uploaded" && x.CompletedAt < DateTimeOffset.UtcNow.AddDays(-7)))
        {
            var full = Path.GetFullPath(item.FilePath); var expected = Path.GetFullPath(Path.Combine(Root, "Recordings")) + Path.DirectorySeparatorChar;
            if (full.StartsWith(expected, StringComparison.OrdinalIgnoreCase) && File.Exists(full)) File.Delete(full);
        }
    }
}
