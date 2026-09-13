using System.Net;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;
namespace Tinrec;
public sealed class ApiException(string message, string code, int status) : Exception(message) { public string Code { get; } = code; public int Status { get; } = status; }
public sealed class ApiClient(LocalStore store, HttpClient? client = null)
{
    private readonly HttpClient http = client ?? new() { Timeout = TimeSpan.FromSeconds(25) };
    public static string ValidateUrl(string value)
    {
        if (!Uri.TryCreate(value.Trim(), UriKind.Absolute, out var uri) || (uri.Scheme != "https" && !(uri.Scheme == "http" && uri.IsLoopback)) || !string.IsNullOrEmpty(uri.UserInfo)) throw new Exception("Informe o endereço HTTPS do seu site Tinrec.");
        return uri.GetLeftPart(UriPartial.Authority);
    }
    public async Task<JsonElement> Request(string path, object? body = null, bool authenticated = true, CancellationToken ct = default)
    {
        var url = ValidateUrl(store.Preferences.SiteUrl);
        using var req = new HttpRequestMessage(body is null ? HttpMethod.Get : HttpMethod.Post, url + "/api" + path);
        if (authenticated) { if (string.IsNullOrEmpty(store.Preferences.Token)) throw new ApiException("Conecte sua conta para enviar vídeos.", "unauthorized", 401); req.Headers.Authorization = new AuthenticationHeaderValue("Bearer", store.Preferences.Token); }
        if (body is not null) req.Content = new StringContent(JsonSerializer.Serialize(body), Encoding.UTF8, "application/json");
        using var response = await http.SendAsync(req, ct);
        var text = await response.Content.ReadAsStringAsync(ct);
        JsonElement json; try { json = JsonSerializer.Deserialize<JsonElement>(text); } catch { throw new Exception("O site não retornou uma resposta válida. Verifique o endereço configurado."); }
        if (!response.IsSuccessStatusCode) throw new ApiException(json.TryGetProperty("message", out var m) ? m.GetString()! : "Falha ao acessar o site.", json.TryGetProperty("code", out var c) ? c.GetString()! : "request_failed", (int)response.StatusCode);
        return json;
    }
    public async Task Reserve(UploadItem item, CancellationToken ct = default)
    {
        if (!string.IsNullOrEmpty(item.Url)) return;
        var response = await Request("/recordings", new { id = item.Id, title = item.Title, resolution = item.Resolution }, ct: ct);
        item.Url = response.GetProperty("url").GetString()!; store.Save();
    }
}
