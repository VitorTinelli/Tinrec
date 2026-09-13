using System.IO;
using System.Net;
using System.Net.Http;
using System.Net.Http.Headers;
namespace Tinrec;
public sealed class UploadWorker(LocalStore store, ApiClient api, HttpClient? client = null)
{
    private readonly SemaphoreSlim gate = new(1, 1);
    private readonly HttpClient http = client ?? new(new HttpClientHandler { AllowAutoRedirect = false }) { Timeout = TimeSpan.FromMinutes(3) };
    public event Action? Changed;
    public bool Busy { get; private set; }
    public async Task Run(CancellationToken ct)
    {
        if (!await gate.WaitAsync(0, ct)) return;
        Busy = true;
        try
        {
            foreach (var item in store.Items.ToArray().Where(x => (x.Status is "queued" or "uploading") && (x.NextAttemptAt is null || x.NextAttemptAt <= DateTimeOffset.UtcNow)))
            {
                try { await Upload(item, ct); }
                catch (OperationCanceledException) when (ct.IsCancellationRequested) { return; }
                catch (Exception e) { item.Status = "queued"; item.Error = e.Message; item.Attempts++; item.NextAttemptAt = DateTimeOffset.UtcNow.AddSeconds(Math.Min(300, 10 * Math.Pow(2, Math.Min(item.Attempts, 5)))); store.Save(); Changed?.Invoke(); if (item.Url.Length > 0 && e is not ApiException { Status: 401 }) { try { var code = e is ApiException failure && new[] { "google_reconnect", "drive_full", "sharing_blocked", "file_unavailable" }.Contains(failure.Code) ? failure.Code : "upload_failed"; await api.Request($"/recordings/{item.Id}/error", new { code }, ct: ct); } catch { } } if (e is ApiException { Status: 401 }) break; }
            }
            try { store.Cleanup(); } catch (IOException) { }
        }
        finally { Busy = false; gate.Release(); }
    }
    private async Task Upload(UploadItem item, CancellationToken ct)
    {
        if (!File.Exists(item.FilePath)) { item.Status = "error"; item.Error = "Arquivo local não encontrado."; store.Save(); Changed?.Invoke(); return; }
        await api.Reserve(item, ct);
        var total = new FileInfo(item.FilePath).Length;
        if (total == 0) throw new Exception("O arquivo de vídeo está vazio.");
        var result = await api.Request($"/recordings/{item.Id}/upload", new { sizeBytes = total, durationMs = item.DurationMs, restart = false }, ct: ct);
        if (result.TryGetProperty("completed", out var done) && done.GetBoolean()) { Finish(item); return; }
        item.UploadUri = result.GetProperty("uri").GetString(); store.Save();
        ValidateUri(item.UploadUri!);
        item.Status = "uploading"; item.Error = null; Changed?.Invoke();
        var offset = await Probe(item.UploadUri!, total, ct);
        if (offset > total) throw new Exception("O Drive informou um tamanho incompatível com o arquivo local.");
        if (offset == -1)
        {
            result = await api.Request($"/recordings/{item.Id}/upload", new { sizeBytes = total, durationMs = item.DurationMs, restart = true }, ct: ct);
            item.UploadUri = result.GetProperty("uri").GetString(); ValidateUri(item.UploadUri!); store.Save(); offset = 0;
        }
        const int chunkSize = 8 * 1024 * 1024;
        using var file = new FileStream(item.FilePath, FileMode.Open, FileAccess.Read, FileShare.Read, chunkSize, FileOptions.Asynchronous | FileOptions.SequentialScan);
        byte[] buffer = new byte[chunkSize];
        while (offset < total)
        {
            ct.ThrowIfCancellationRequested(); file.Position = offset;
            var count = (int)Math.Min(chunkSize, total - offset); await file.ReadExactlyAsync(buffer.AsMemory(0, count), ct);
            using var req = new HttpRequestMessage(HttpMethod.Put, item.UploadUri);
            req.Content = new ByteArrayContent(buffer, 0, count); req.Content.Headers.ContentType = new MediaTypeHeaderValue("video/mp4"); req.Content.Headers.ContentRange = new ContentRangeHeaderValue(offset, offset + count - 1, total);
            using var response = await http.SendAsync(req, HttpCompletionOption.ResponseHeadersRead, ct);
            if ((int)response.StatusCode == 308)
            {
                var next = Offset(response); if (next <= offset || next > offset + count) throw new Exception("O Drive não confirmou o bloco enviado. O envio será retomado."); offset = next;
            }
            else if (response.IsSuccessStatusCode) offset = total;
            else if (response.StatusCode is HttpStatusCode.NotFound or HttpStatusCode.Gone) { item.UploadUri = null; throw new Exception("Sessão expirada. O envio será reiniciado automaticamente."); }
            else throw new Exception(response.StatusCode == HttpStatusCode.Forbidden ? "O Google Drive recusou o envio. Verifique espaço e permissões." : "Envio interrompido. Tentaremos novamente automaticamente.");
            item.UploadedBytes = offset; store.Save(); Changed?.Invoke();
            await api.Request($"/recordings/{item.Id}/progress", new { uploadedBytes = offset }, ct: ct);
        }
        await api.Request($"/recordings/{item.Id}/complete", new { }, ct: ct); Finish(item);
    }
    private void Finish(UploadItem item) { item.Status = "uploaded"; item.Error = null; item.UploadUri = null; item.Attempts = 0; item.NextAttemptAt = null; item.CompletedAt = DateTimeOffset.UtcNow; store.Save(); Changed?.Invoke(); }
    private static void ValidateUri(string value) { var uri = new Uri(value); if (uri.Scheme != "https" || uri.Host != "www.googleapis.com" || !uri.AbsolutePath.StartsWith("/upload/drive/")) throw new Exception("Endereço de upload inválido."); }
    private static long Offset(HttpResponseMessage response) { if (!response.Headers.TryGetValues("Range", out var values)) return 0; var range = values.First().Split('-'); return long.TryParse(range.Last(), out var last) ? last + 1 : throw new Exception("Progresso retornado pelo Drive é inválido."); }
    private async Task<long> Probe(string uri, long size, CancellationToken ct)
    {
        using var req = new HttpRequestMessage(HttpMethod.Put, uri) { Content = new ByteArrayContent([]) };
        req.Content.Headers.ContentRange = new ContentRangeHeaderValue(size);
        using var response = await http.SendAsync(req, HttpCompletionOption.ResponseHeadersRead, ct);
        if (response.IsSuccessStatusCode) return size;
        if ((int)response.StatusCode == 308) return Offset(response);
        if (response.StatusCode is HttpStatusCode.NotFound or HttpStatusCode.Gone) return -1;
        throw new Exception("Não foi possível consultar o envio no Drive. Tentaremos novamente.");
    }
}
