#!/usr/bin/env python3
"""Native Tauri UI -> IPC -> capture engine -> FFmpeg -> MP4 smoke test.

Needs tauri-driver and WebKitWebDriver (Linux). The default test requires a
debug binary and refuses to record unless the app confirms its synthetic source.
Use --native explicitly to test real capture and approve the system dialog.
"""
import argparse
import base64
import json
import os
import re
from pathlib import Path
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--binary", type=Path, default=Path("desktop-app/src-tauri/target/debug/tinrec-desktop"))
parser.add_argument("--driver", default="tauri-driver")
parser.add_argument("--native-driver")
parser.add_argument("--port", type=int, default=4444)
parser.add_argument("--native", action="store_true")
parser.add_argument("--background",action="store_true",help="Test close-to-tray, reopen and graceful quit (Linux synthetic mode)")
parser.add_argument("--controls", action="store_true", help="Exercise preview, floating-window controls and discard (synthetic mode only)")
parser.add_argument("--artifacts", type=Path, default=Path("artifacts/desktop-tests"))
args = parser.parse_args()
if (args.controls or args.background) and args.native:
    parser.error("--controls uses only the synthetic test source; do not combine it with --native")
args.artifacts.mkdir(parents=True, exist_ok=True)
root = f"http://127.0.0.1:{args.port}"
session = None


def request(method, route, data=None):
    payload = json.dumps(data).encode() if data is not None else None
    req = urllib.request.Request(root + route, data=payload, headers={"Content-Type": "application/json"}, method=method)
    try:
        with urllib.request.urlopen(req, timeout=30) as response:
            return json.load(response)["value"]
    except urllib.error.HTTPError as error:
        raise RuntimeError(error.read().decode()) from error


def command(method, route, data=None):
    return request(method, f"/session/{session}" + route, data)


def javascript(script):
    return command("POST", "/execute/sync", {"script": script, "args": []})


def invoke(command_name):
    return command("POST","/execute/async",{"script":"const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke("+json.dumps(command_name)+").then(value=>done({ok:true,value}),error=>done({ok:false,error:String(error)}));","args":[]})


def element(selector):
    result = command("POST", "/element", {"using": "css selector", "value": selector})
    return result["element-6066-11e4-a52e-4f735466cecf"]


def click(selector):
    command("POST", f"/element/{element(selector)}/click", {})


def navigate(name):
    selector = f'//nav[@aria-label="Espaço de trabalho"]/button[starts-with(normalize-space(.), {json.dumps(name)})]'
    result = command("POST", "/element", {"using": "xpath", "value": selector})
    target = result["element-6066-11e4-a52e-4f735466cecf"]
    command("POST", f"/element/{target}/click", {})
    wait_for(f"return [...document.querySelectorAll('.workspace-nav > button')].some(button => button.textContent.trim().startsWith({json.dumps(name)}) && button.getAttribute('aria-current') === 'page')", f"navigation to {name}")


def set_title(value):
    navigate("Configurações")
    wait_for("const input=document.querySelector('#title');return input && !input.disabled && input.getClientRects().length > 0", "editable recording title")
    title = element("#title")
    command("POST", f"/element/{title}/clear", {})
    command("POST", f"/element/{title}/value", {"text": value})


def screenshot(name):
    args.artifacts.joinpath(name + ".png").write_bytes(base64.b64decode(command("GET", "/screenshot")))


def wait_for(test, label, timeout=30):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if javascript(test):
            return
        issue = javascript("return document.querySelector('[role=alert]')?.innerText")
        if issue:
            raise AssertionError(issue)
        time.sleep(0.15)
    raise AssertionError(f"Timed out: {label}")


def state(label):
    wait_for(f"return document.querySelector('.recording-status')?.textContent === {json.dumps(label)}", label, 180 if args.native else 30)


with tempfile.TemporaryDirectory(prefix="tinrec-desktop-") as data_dir:
    env = os.environ.copy()
    env["TINREC_DATA_DIR"] = data_dir
    env.pop("TINREC_TEST_SYNTHETIC", None)
    if not args.native:
        env["TINREC_TEST_SYNTHETIC"] = "1"
    driver_args = [args.driver, "--port", str(args.port), "--native-port", str(args.port + 1)]
    if args.native_driver:
        driver_args.extend(["--native-driver", args.native_driver])
    with args.artifacts.joinpath("driver.log").open("w") as log:
        driver = subprocess.Popen(driver_args, env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 15
            while True:
                try:
                    request("GET", "/status")
                    break
                except (OSError, RuntimeError):
                    if driver.poll() is not None or time.monotonic() >= deadline:
                        raise RuntimeError("WebDriver did not start; inspect driver.log")
                    time.sleep(0.2)
            capabilities = {"capabilities": {"alwaysMatch": {"tauri:options": {"application": str(args.binary.resolve())}}}}
            session = request("POST", "/session", capabilities)["sessionId"]
            command("POST", "/timeouts", {"implicit": 10000})
            wait_for("return !!document.querySelector('button[data-capture-mode=screen]') && !document.querySelector('button[data-capture-mode=screen]').disabled", "app ready")
            synthetic = javascript("return document.querySelector('.app-shell').dataset.synthetic === 'true'")
            assert args.native or synthetic, "Refusing real capture: use a debug binary or explicitly pass --native"
            screenshot("ready")
            main_handle=command("GET","/window")
            controls_handle=None
            def floating():
                global controls_handle
                if controls_handle is None:
                    for handle in command("GET","/window/handles"):
                        command("POST","/window",{"handle":handle})
                        if javascript("return new URLSearchParams(location.search).get('window')") == "controls":
                            controls_handle=handle
                            break
                    assert controls_handle, "Floating recording window not found"
                command("POST","/window",{"handle":controls_handle})
            reports = []
            for index in range(0 if args.background else (1 if args.native else 2)):
                set_title(f"Teste da interface {index + 1}")
                navigate("Gravador")
                click("button[data-capture-mode=screen]")
                state("Gravando")
                time.sleep(1.1)
                if args.controls and index==0:
                    screenshot("recording")
                    floating()
                    wait_for("return !document.querySelector('button[aria-label=\"Silenciar microfone\"]')?.disabled", "floating controls ready")
                    click('button[aria-label="Silenciar microfone"]')
                    wait_for("return !!document.querySelector('button[aria-label=\"Ativar microfone\"]')", "microphone muted")
                    click('button[aria-label="Silenciar áudio do computador"]')
                    wait_for("return !!document.querySelector('button[aria-label=\"Ativar áudio do computador\"]')", "system audio muted")
                    screenshot("floating-muted")
                    click('button[aria-label="Ativar microfone"]')
                    wait_for("return !!document.querySelector('button[aria-label=\"Silenciar microfone\"]')", "microphone restored")
                    click('button[aria-label="Ativar áudio do computador"]')
                    wait_for("return !!document.querySelector('button[aria-label=\"Silenciar áudio do computador\"]')", "system audio restored")
                    click('button[aria-label="Pausar gravação"]')
                    command("POST","/window",{"handle":main_handle})
                else:
                    click(".record-actions .secondary")
                state("Gravação pausada")
                before = javascript("return document.querySelector('.timer').textContent")
                time.sleep(1.1)
                assert javascript("return document.querySelector('.timer').textContent") == before, "Timer moved while paused"
                screenshot(f"paused-{index + 1}")
                click(".record-actions .secondary")
                state("Gravando")
                time.sleep(1.1)
                click(".record-actions .stop-button")
                state("Vídeo salvo")
                wait_for(f"return document.querySelectorAll('.video-card').length >= {index + 1}", "recording in library")
                navigate("Meus vídeos")
                wait_for("return document.querySelector('.video-thumbnail img')?.naturalWidth > 0", "video thumbnail")
                screenshot(f"completed-{index + 1}")
                navigate("Gravador")
                paths = sorted(Path(data_dir, "Recordings").glob("*.capture.json"), key=lambda path: path.stat().st_mtime_ns)
                assert len(paths) == index + 1, f"Expected {index + 1} reports, got {paths}"
                report = json.loads(paths[-1].read_text())
                assert report["status"] == "completed", report.get("error")
                assert report["checks"]["passed"], report["checks"]
                assert report["mode"] == ("native" if args.native else "synthetic")
                args.artifacts.joinpath(f"recording-{index + 1}.json").write_text(json.dumps(report, indent=2, ensure_ascii=False))
                source = paths[-1].with_suffix("").with_suffix("")
                assert source.is_file(), source
                assert not Path(str(source) + ".partial.mp4").exists()
                reports.append(report["checks"])
            if args.controls:
                assert not args.native, "Discard smoke test is restricted to the synthetic source"
                set_title("Teste de descarte")
                navigate("Gravador")
                click("button[data-capture-mode=screen]");state("Gravando")
                wait_for("return document.querySelector('.preview-viewport img')?.naturalWidth > 0", "preview before discard")
                floating();click('button[aria-label="Descartar gravação"]')
                wait_for("return !!document.querySelector('[role=alertdialog]')", "discard confirmation")
                screenshot("discard-confirmation")
                click(".confirm-delete")
                command("POST","/window",{"handle":main_handle});state("Vídeo descartado")
                assert not list(Path(data_dir,"Recordings").glob("Teste de descarte*")), "Discarded capture left files behind"
                assert len(list(Path(data_dir,"Recordings").glob("*.mp4")))==len(reports), "Discard removed a previous recording or left a new file"
                screenshot("discarded")
            if args.background:
                names=subprocess.check_output(["gdbus","call","--session","--dest","org.freedesktop.DBus","--object-path","/org/freedesktop/DBus","--method","org.freedesktop.DBus.ListNames"],text=True)
                tray_names=re.findall(r"'(org\.kde\.StatusNotifierItem-[^']+)'",names)
                assert len(tray_names)==1, f"Expected one test tray service, got {tray_names}"
                tray=tray_names[0]
                def tray_call(path,method,*values):
                    return subprocess.check_output(["gdbus","call","--session","--dest",tray,"--object-path",path,"--method",method,"--",*values],text=True,timeout=15)
                assert "Tinrec" in tray_call("/StatusNotifierItem","org.freedesktop.DBus.Properties.Get","org.kde.StatusNotifierItem","Title")
                assert invoke("hide_to_tray")["ok"]
                time.sleep(.3)
                assert invoke("get_snapshot")["ok"], "Closing idle main window killed the app"
                tray_call("/StatusNotifierItem","org.kde.StatusNotifierItem.Activate","0","0")
                wait_for("return document.visibilityState === 'visible'", "main window reopened from tray")
                set_title("Teste em segundo plano")
                navigate("Gravador")
                click("button[data-capture-mode=screen]");state("Gravando");assert invoke("hide_to_tray")["ok"]
                first=invoke("get_snapshot")["value"];time.sleep(.8);second=invoke("get_snapshot")["value"]
                assert second["phase"]=="recording" and second["elapsedSeconds"]>first["elapsedSeconds"]+.3, "Closing main window stopped the capture"
                layout=tray_call("/MenuBar","com.canonical.dbusmenu.GetLayout","0","-1","[]")
                args.artifacts.joinpath("tray-layout.txt").write_text(layout)
                quit_item=re.search(r"\((\d+), \{[^}]*'label': <'Sair do Tinrec'>",layout)
                assert quit_item, f"Quit item missing: {layout}"
                tray_call("/MenuBar","com.canonical.dbusmenu.Event",quit_item[1],"clicked","<0>","0")
                deadline=time.monotonic()+20
                while True:
                    paths=list(Path(data_dir,"Recordings").glob("Teste em segundo plano*.capture.json"))
                    if paths:
                        last=json.loads(paths[0].read_text());assert last["status"]=="completed",last.get("error");assert last["checks"]["passed"];break
                    assert time.monotonic()<deadline,"Quit did not finalize the active recording"
                    time.sleep(.2)
                args.artifacts.joinpath("background-quit-report.json").write_text(json.dumps(last,indent=2))
                time.sleep(.3)
                names=subprocess.check_output(["gdbus","call","--session","--dest","org.freedesktop.DBus","--object-path","/org/freedesktop/DBus","--method","org.freedesktop.DBus.ListNames"],text=True)
                assert tray not in names,"Explicit quit left the tray service running"
            print(json.dumps({"native_window": True,"preview_and_floating_controls": args.controls,"discarded_files_removed":args.controls,"background_lifecycle":args.background, "mode": "native" if args.native else "synthetic", "sessions": reports}, indent=2))
        except Exception:
            if session:
                try:
                    screenshot("failure")
                except (OSError, RuntimeError):
                    pass
            raise
        finally:
            if session:
                try:
                    javascript("void window.__TAURI_INTERNALS__.invoke('quit_app').catch(()=>{});return true")
                except (OSError, RuntimeError):
                    pass
                try:
                    request("DELETE", f"/session/{session}")
                except (OSError, RuntimeError):
                    pass
            driver.terminate()
            try:
                driver.wait(timeout=10)
            except subprocess.TimeoutExpired:
                driver.kill()
                driver.wait()
