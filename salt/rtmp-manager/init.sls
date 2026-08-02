{% set ffmpeg = pillar['rtmp_proxy']['ffmpeg'] %}

{{ ffmpeg['install_dir'] }}:
  archive.extracted:
    - source: {{ ffmpeg['source_url'] }}
    - skip_verify: true
    - use_etag: true
    - overwrite: true
    - archive_format: tar
    - user: root
    - group: root
    - enforce_ownership_on: {{ ffmpeg['install_dir'] }}

/usr/local/bin/ffmpeg:
  file.symlink:
    - target: {{ ffmpeg['install_dir'] }}/{{ ffmpeg['archive_dir'] }}/bin/ffmpeg
    - require:
      - archive: {{ ffmpeg['install_dir'] }}

/usr/local/bin/ffprobe:
  file.symlink:
    - target: {{ ffmpeg['install_dir'] }}/{{ ffmpeg['archive_dir'] }}/bin/ffprobe
    - require:
      - archive: {{ ffmpeg['install_dir'] }}

/opt/apps/rtmp-proxy/current:
  file.directory:
    - user: root
    - group: root
    - mode: '0755'
    - makedirs: true

/opt/apps/rtmp-proxy/shared:
  file.directory:
    - user: root
    - group: root
    - mode: '0700'
    - makedirs: true

/opt/apps/rtmp-proxy/current/rtmp-proxy:
  file.managed:
    - source: {{ pillar['rtmp_proxy']['release_url'] }}
    - source_hash: {{ pillar['rtmp_proxy']['release_url'] }}.sha256
    - user: root
    - group: root
    - mode: '0755'
    - require:
      - file: /opt/apps/rtmp-proxy/current

/opt/apps/rtmp-proxy/shared/config.json:
  file.managed:
    - contents_pillar: rtmp_proxy:config
    - user: root
    - group: root
    - mode: '0600'
    - show_changes: false
    - replace: false
    - require:
      - file: /opt/apps/rtmp-proxy/shared

/etc/rtmp-proxy.env:
  file.managed:
    - contents_pillar: rtmp_proxy:environment
    - user: root
    - group: root
    - mode: '0600'
    - show_changes: false

/etc/systemd/system/rtmp-proxy.service:
  file.managed:
    - source: salt://rtmp-manager/files/rtmp-proxy.service
    - user: root
    - group: root
    - mode: '0644'

reload-systemd-for-rtmp-proxy:
  cmd.run:
    - name: systemctl daemon-reload
    - onchanges:
      - file: /etc/systemd/system/rtmp-proxy.service

/etc/caddy/apps/rtmp-proxy.caddy:
  file.managed:
    - source: salt://rtmp-manager/files/rtmp-proxy.caddy
    - user: root
    - group: root
    - mode: '0644'

caddy.service:
  service.running:
    - enable: true
    - reload: true
    - watch:
      - file: /etc/caddy/apps/rtmp-proxy.caddy

rtmp-proxy.service:
  service.running:
    - enable: true
    - watch:
      - file: /opt/apps/rtmp-proxy/current/rtmp-proxy
      - file: /etc/rtmp-proxy.env
      - file: /etc/systemd/system/rtmp-proxy.service
    - require:
      - file: /usr/local/bin/ffmpeg
      - file: /opt/apps/rtmp-proxy/shared/config.json
      - cmd: reload-systemd-for-rtmp-proxy

verify-rtmp-proxy-health:
  cmd.run:
    - name: curl --fail --silent --show-error --retry 10 --retry-delay 1 http://127.0.0.1:8080/
    - onchanges:
      - file: /opt/apps/rtmp-proxy/current/rtmp-proxy
      - file: /etc/rtmp-proxy.env
      - file: /etc/systemd/system/rtmp-proxy.service
    - require:
      - service: rtmp-proxy.service
