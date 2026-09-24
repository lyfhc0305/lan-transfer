"""0.1 command-line prototype only (not the desktop app). Local integration checks and macOS RSS/CPU sampling; no external packages."""
import hashlib, json, os, platform, socket, subprocess, tempfile, threading, time
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / 'target/release/lan-transfer'

def sha(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''): h.update(block)
    return h.hexdigest()

def stat(pid):
    p = subprocess.run(['ps', '-o', 'rss=,time=', '-p', str(pid)], capture_output=True, text=True)
    parts = p.stdout.split()
    if len(parts) != 2: return None
    minutes, seconds = parts[1].split(':')
    return {'rss_mib': int(parts[0]) / 1024, 'cpu_seconds': int(minutes) * 60 + float(seconds)}

def main():
    result = {'platform': platform.platform(), 'architecture': platform.machine(), 'scope': 'Release Rust core, one Mac loopback; no UI, not LAN or Windows measurements', 'checks': []}
    key = subprocess.check_output([str(BIN), 'keygen'], text=True).strip()
    env = dict(os.environ, LAN_TRANSFER_KEY=key)
    with tempfile.TemporaryDirectory(prefix='lan-transfer-test-') as tmp:
        tmp = Path(tmp); recv = tmp/'receive'; recv.mkdir()
        with socket.socket() as s: s.bind(('127.0.0.1', 0)); port = s.getsockname()[1]
        addr = f'127.0.0.1:{port}'
        with open(tmp/'server.log', 'w+') as log:
            server = subprocess.Popen([str(BIN), 'receive', str(recv), addr, 'Test Mac'], env=env, stdout=log, stderr=log)
            try:
                for _ in range(100):
                    try:
                        with socket.create_connection(('127.0.0.1',port),timeout=.1): pass
                        break
                    except OSError: time.sleep(.05)
                else: raise RuntimeError('Server did not start')
                discovery = subprocess.check_output([str(BIN), 'discover', addr], text=True)
                assert addr in discovery; result['checks'].append('UDP discovery (unicast query over loopback)')
                samples=[]; start=time.monotonic(); first=stat(server.pid)
                for _ in range(15): time.sleep(1); samples.append(stat(server.pid))
                last=samples[-1]; elapsed=time.monotonic()-start
                result['idle']={'seconds':round(elapsed,2),'rss_mib':round(max(x['rss_mib'] for x in samples),2),'average_cpu_percent':round((last['cpu_seconds']-first['cpu_seconds'])/elapsed*100,3)}
                small=tmp/'中文测试.txt'; small.write_text('Mac 与 Windows 文件互传\n' * 100)
                def send(path, use_env=env): return subprocess.run([str(BIN),'send',addr,str(path)],env=use_env,text=True,capture_output=True,timeout=90)
                assert send(small).returncode==0; assert sha(small)==sha(recv/small.name)
                assert send(small).returncode==0; assert sha(small)==sha(recv/('1-'+small.name))
                result['checks'] += ['Unicode filename and SHA-256 integrity', 'Duplicate filename does not overwrite']
                empty=tmp/'empty';empty.touch();assert send(empty).returncode==0;assert (recv/'empty').stat().st_size==0
                result['checks'].append('Zero-byte file')
                wrong=dict(env,LAN_TRANSFER_KEY='00'*32); assert send(small,wrong).returncode != 0
                assert not (recv/('2-'+small.name)).exists();result['checks'].append('Wrong key rejected')
                # Partial handshake must not crash the receiver or create a final file.
                with socket.create_connection(('127.0.0.1',port)) as s: s.sendall(b'incomplete')
                assert send(small).returncode==0;result['checks'].append('Recovery after interrupted connection')
                result['transfers']=[]
                for mib in [64,512]:
                    path=tmp/f'{mib}MiB.bin'; block=os.urandom(1024*1024)
                    with open(path,'wb') as f:
                        for _ in range(mib): f.write(block)
                    rss=[];stop=threading.Event()
                    def sample():
                        while not stop.is_set():
                            v=stat(server.pid)
                            if v:rss.append(v['rss_mib'])
                            stop.wait(.025)
                    before=stat(server.pid);worker=threading.Thread(target=sample);worker.start();t=time.monotonic()
                    p=send(path);duration=time.monotonic()-t;stop.set();worker.join();after=stat(server.pid)
                    assert p.returncode==0,p.stderr;assert sha(path)==sha(recv/path.name)
                    result['transfers'].append({'size_mib':mib,'seconds':round(duration,3),'throughput_mib_s':round(mib/duration,1),'receiver_peak_sampled_rss_mib':round(max(rss or [after['rss_mib']]),2),'receiver_average_cpu_percent':round((after['cpu_seconds']-before['cpu_seconds'])/duration*100,1)})
                    path.unlink();(recv/path.name).unlink()
                result['checks'].append('64 MiB and 512 MiB encrypted transfers match SHA-256')
                result['binary_mib']=round(BIN.stat().st_size/1024/1024,2)
            finally:
                server.terminate()
                try:server.wait(timeout=3)
                except subprocess.TimeoutExpired:server.kill();server.wait()
    out=ROOT/'docs/benchmark-macos.json';out.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print(out.read_text())
if __name__=='__main__':main()
