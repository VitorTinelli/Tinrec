using Tinrec;
using ScreenRecorderLib;
using System.Diagnostics;
using System.IO;
using System.Text.Json;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Threading;
using Brushes=System.Windows.Media.Brushes;

internal static class Program
{
 [STAThread]static void Main(string[] args)
 {
  if(args.Contains("--ui")){RunUi();return;}
  var app=new System.Windows.Application();
  var window=new Window{Title="Tinrec — teste de captura",Width=850,Height=520,Background=new SolidColorBrush(System.Windows.Media.Color.FromRgb(246,242,255)),WindowStartupLocation=WindowStartupLocation.CenterScreen};
  var text=new TextBlock{FontSize=36,Foreground=Brushes.MediumPurple,HorizontalAlignment=System.Windows.HorizontalAlignment.Center,VerticalAlignment=VerticalAlignment.Center,Text="Tinrec · diagnóstico"};window.Content=text;
  window.Loaded+=async(_,_)=>{
   int duration=args.Length>0?int.Parse(args[0]):8;
   var output=Path.GetFullPath(args.Length>1?args[1]:"artifacts/capture-test.mp4");Directory.CreateDirectory(Path.GetDirectoryName(output)!);
   var monitor=System.Windows.Forms.Screen.FromHandle(new System.Windows.Interop.WindowInteropHelper(window).Handle);
   var origin=window.PointToScreen(new System.Windows.Point(0,0));var end=window.PointToScreen(new System.Windows.Point(window.ActualWidth,window.ActualHeight));
   ScreenRect? region=args.Contains("--full")?null:new ScreenRect(Math.Max(0,origin.X-monitor.Bounds.X),Math.Max(0,origin.Y-monitor.Bounds.Y),Math.Min(end.X-origin.X,monitor.Bounds.Right-origin.X)/2*2,Math.Min(end.Y-origin.Y,monitor.Bounds.Bottom-origin.Y)/2*2);
   var prefs=new Preferences{Resolution=args.Contains("--720")?720:1080,Microphone=args.Contains("--mic"),SystemAudio=args.Contains("--system")};
   using var capture=new CaptureService();var process=Process.GetCurrentProcess();var samples=new List<object>();var sw=Stopwatch.StartNew();var startCpu=process.TotalProcessorTime;
   var ticker=new DispatcherTimer{Interval=TimeSpan.FromSeconds(1)};
   ticker.Tick+=(_,_)=>{process.Refresh();text.Text=$"Tinrec · {capture.ElapsedMs/1000:0}s\n{DateTime.Now:HH:mm:ss}";text.Foreground=(capture.ElapsedMs/1000)%2==0?Brushes.MediumPurple:Brushes.Teal;if((int)sw.Elapsed.TotalSeconds%10==0)samples.Add(new{seconds=sw.Elapsed.TotalSeconds,workingSet=process.WorkingSet64,privateBytes=process.PrivateMemorySize64,frames=capture.Frames,cpuSeconds=process.TotalProcessorTime.TotalSeconds});};
   try{
    await capture.Start(output,monitor.DeviceName,region,prefs);ticker.Start();
    Console.WriteLine("Recording diagnostics started: "+output);
    if(args.Contains("--pause")){await Task.Delay(2000);capture.TogglePause();await Task.Delay(2000);capture.TogglePause();await Task.Delay(Math.Max(1,duration-2)*1000);}else await Task.Delay(duration*1000);
    await capture.Stop();ticker.Stop();process.Refresh();
    var report=new{file=output,durationMs=capture.ElapsedMs,frames=capture.Frames,wallSeconds=sw.Elapsed.TotalSeconds,cpuPercent=(process.TotalProcessorTime-startCpu).TotalSeconds/sw.Elapsed.TotalSeconds/Environment.ProcessorCount*100,workingSet=process.WorkingSet64,privateBytes=process.PrivateMemorySize64,size=new FileInfo(output).Length,samples};
    await File.WriteAllTextAsync(output+".json",JsonSerializer.Serialize(report,new JsonSerializerOptions{WriteIndented=true}));Console.WriteLine(JsonSerializer.Serialize(report));
   }catch(Exception e){Console.Error.WriteLine(e);Environment.ExitCode=1;}
   finally{ticker.Stop();app.Shutdown();}
  };
  app.Run(window);
 }
 private static void RunUi()
 {
  var app=new System.Windows.Application();
  System.Xml.Linq.XNamespace ns="http://schemas.microsoft.com/winfx/2006/xaml/presentation";
  var resources=System.Xml.Linq.XDocument.Load("desktop/Tinrec/App.xaml").Root!.Element(ns+"Application.Resources")!;
  var dictionary=new System.Xml.Linq.XElement(ns+"ResourceDictionary",new System.Xml.Linq.XAttribute(System.Xml.Linq.XNamespace.Xmlns+"x","http://schemas.microsoft.com/winfx/2006/xaml"),resources.Elements());
  app.Resources=(ResourceDictionary)System.Windows.Markup.XamlReader.Parse(dictionary.ToString());
  var window=new MainWindow(new LocalStore(Path.GetFullPath("artifacts/ui-test")));
  window.Loaded+=async(_,_)=>{
   try{
    await Task.Delay(800);window.UpdateLayout();
    var monitor=(System.Windows.Controls.ComboBox)window.FindName("Displays");
    if(monitor.Items.Count==0)throw new Exception("No displays loaded");
    var image=new System.Windows.Media.Imaging.RenderTargetBitmap((int)window.ActualWidth,(int)window.ActualHeight,96,96,PixelFormats.Pbgra32);image.Render(window);
    var encoder=new System.Windows.Media.Imaging.PngBitmapEncoder();encoder.Frames.Add(System.Windows.Media.Imaging.BitmapFrame.Create(image));
    using(var file=File.Create(Path.GetFullPath("artifacts/desktop-ui.png")))encoder.Save(file);
    Console.WriteLine("UI smoke passed; monitors: "+monitor.Items.Count);
   }catch(Exception e){Console.Error.WriteLine(e);Environment.ExitCode=1;}
   finally{window.Close();app.Shutdown();}
  };
  app.Run(window);
 }
}
