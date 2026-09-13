using System.Net;
using System.Net.Http;
using System.IO;
using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;
using Tinrec;

static void Assert(bool condition,string name){if(!condition)throw new Exception("FAIL: "+name);Console.WriteLine("PASS: "+name);}
static HttpResponseMessage Json(object data,HttpStatusCode status=HttpStatusCode.OK)=>new(status){Content=new StringContent(JsonSerializer.Serialize(data),Encoding.UTF8,"application/json")};
static HttpResponseMessage Incomplete(long bytes){var r=new HttpResponseMessage((HttpStatusCode)308);if(bytes>0)r.Headers.TryAddWithoutValidation("Range",$"bytes=0-{bytes-1}");return r;}
var root=Path.GetFullPath(Path.Combine("artifacts","desktop-tests",Guid.NewGuid().ToString()));
var store=new LocalStore(root);
store.Preferences.Token="test-device-secret";store.Preferences.SiteUrl="https://tinrec.test";
var item=new UploadItem{Title="Upload retomado",Status="queued",DurationMs=5000,FilePath=Path.Combine(root,"Recordings","sample.mp4")};
const int chunk=8*1024*1024;
await File.WriteAllBytesAsync(item.FilePath,new byte[chunk+17]);store.Items.Add(item);store.Save();
Assert(!Encoding.UTF8.GetString(await File.ReadAllBytesAsync(Path.Combine(root,"preferences.bin"))).Contains("test-device-secret"),"Credencial local protegida por DPAPI");
var loaded=new LocalStore(root);Assert(loaded.Preferences.Token=="test-device-secret"&&loaded.Items.Single().Id==item.Id,"Fila e credenciais sobrevivem ao reinício");
long serverOffset=chunk;int chunks=0,completions=0,starts=0;bool expire=false,denyComplete=false,failChunk=false;
var handler=new StubHandler(async req=>{
 var path=req.RequestUri!.AbsolutePath;
 if(req.RequestUri.Host=="www.googleapis.com"){
  var range=req.Content!.Headers.ContentRange!;
  if(!range.HasRange){if(expire){expire=false;return new HttpResponseMessage(HttpStatusCode.Gone);}return Incomplete(serverOffset);}
  if(failChunk){failChunk=false;throw new HttpRequestException("Simulated network failure");}
  Assert(range.From==serverOffset,"Retoma exatamente do offset confirmado pelo Drive");
  var bytes=await req.Content.ReadAsByteArrayAsync();Assert(bytes.Length==range.To-range.From+1,"Envia somente o bloco necessário");chunks++;serverOffset=range.To!.Value+1;return serverOffset==range.Length?Json(new{id="drive-file"}):Incomplete(serverOffset);
 }
 Assert(req.Headers.Authorization?.Parameter=="test-device-secret","API recebe token do dispositivo");
 if(path=="/api/recordings")return Json(new{url="https://tinrec.test/v/public"});
 if(path.EndsWith("/upload")){starts++;var body=JsonSerializer.Deserialize<JsonElement>(await req.Content!.ReadAsStringAsync());if(body.GetProperty("restart").GetBoolean())serverOffset=0;return Json(new{uri="https://www.googleapis.com/upload/drive/v3/files/file?upload_id=test"});}
 if(path.EndsWith("/progress"))return Json(new{ok=true});
 if(path.EndsWith("/error"))return Json(new{ok=true});
 if(path.EndsWith("/complete")){completions++;return denyComplete?Json(new{code="sharing_blocked",message="Sharing denied"},HttpStatusCode.Conflict):Json(new{ok=true});}
 throw new Exception("Unexpected request "+path);
});
var http=new HttpClient(handler);var api=new ApiClient(store,http);var worker=new UploadWorker(store,api,http);
await worker.Run(CancellationToken.None);
Assert(item.Status=="uploaded"&&chunks==1&&completions==1,"Upload retoma sem reenviar o primeiro bloco");
Assert(File.Exists(item.FilePath),"Arquivo local mantido após confirmação");
item.Status="queued";item.CompletedAt=null;item.NextAttemptAt=null;expire=true;chunks=0;serverOffset=0;
await worker.Run(CancellationToken.None);
Assert(item.Status=="uploaded"&&chunks==2&&starts==3,"Sessão expirada é recriada e os blocos são reenviados");
item.Status="queued";item.CompletedAt=null;item.NextAttemptAt=null;serverOffset=0;denyComplete=true;
await worker.Run(CancellationToken.None);
Assert(item.Status=="queued"&&item.NextAttemptAt>DateTimeOffset.UtcNow&&File.Exists(item.FilePath),"Compartilhamento bloqueado preserva arquivo e agenda nova tentativa");
denyComplete=false;failChunk=true;item.NextAttemptAt=null;serverOffset=0;
await worker.Run(CancellationToken.None);
Assert(item.Status=="queued"&&item.Attempts>1&&File.Exists(item.FilePath),"Queda de rede preserva arquivo e usa backoff");
item.NextAttemptAt=null;await worker.Run(CancellationToken.None);Assert(item.Status=="uploaded"&&item.Attempts==0,"Nova tentativa conclui e limpa erro");
item.CompletedAt=DateTimeOffset.UtcNow.AddDays(-8);store.Cleanup();Assert(!File.Exists(item.FilePath),"Limpeza remove apenas arquivos enviados há sete dias");
var pending=new UploadItem{Status="queued",FilePath=Path.Combine(root,"Recordings","pending.mp4"),CompletedAt=DateTimeOffset.UtcNow.AddDays(-8)};await File.WriteAllTextAsync(pending.FilePath,"keep");store.Items.Add(pending);store.Cleanup();Assert(File.Exists(pending.FilePath),"Limpeza preserva envios pendentes");
var outside=Path.Combine(root,"outside.mp4");await File.WriteAllTextAsync(outside,"keep");store.Items.Add(new UploadItem{Status="uploaded",FilePath=outside,CompletedAt=DateTimeOffset.UtcNow.AddDays(-8)});store.Cleanup();Assert(File.Exists(outside),"Limpeza não remove arquivos fora da pasta de gravações");
Assert(ApiClient.ValidateUrl("http://localhost:5173/path")=="http://localhost:5173","Permite servidor local de desenvolvimento");
try{ApiClient.ValidateUrl("http://example.com");throw new Exception("FAIL: insecure URL");}catch(Exception e)when(!e.Message.StartsWith("FAIL:")){Console.WriteLine("PASS: Recusa site HTTP externo");}
Console.WriteLine("Desktop queue tests complete.");

sealed class StubHandler(Func<HttpRequestMessage,Task<HttpResponseMessage>> send):HttpMessageHandler
{
 protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request,CancellationToken cancellationToken)=>send(request);
}
