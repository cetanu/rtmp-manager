#!/usr/bin/python3
from pathlib import Path
from deployment import config, release, run, service, unit

HERE = Path(__file__).resolve().parent
app = config(HERE)['rtmp_proxy']
ffmpeg = app['ffmpeg']
ffmpeg_changed = release(ffmpeg['source_url'], ffmpeg['source_hash'], '/opt/ffmpeg', ffmpeg['archive_dir'] + '/bin/ffmpeg')
for binary in ['ffmpeg', 'ffprobe']:
    target = Path('/usr/local/bin') / binary
    source = Path('/opt/ffmpeg/current') / ffmpeg['archive_dir'] / 'bin' / binary
    if not target.is_symlink() or target.readlink() != source:
        next_link = target.with_name('.' + binary + '.next')
        next_link.unlink(missing_ok=True)
        next_link.symlink_to(source)
        next_link.replace(target)
shared = Path('/opt/apps/rtmp-proxy/shared')
shared.mkdir(parents=True, exist_ok=True)
shared.chmod(0o700)
changed = release(app['release_url'], app['release_url'] + '.sha256', '/opt/apps/rtmp-proxy', 'rtmp-proxy')
changed |= ffmpeg_changed
changed |= unit(HERE / 'files/rtmp-proxy.service', 'rtmp-proxy.service')
service('rtmp-proxy.service', changed, ['/opt/apps/rtmp-proxy/current/rtmp-proxy', '/usr/local/bin/ffmpeg', '/usr/local/bin/ffprobe', '/etc/systemd/system/rtmp-proxy.service'])
