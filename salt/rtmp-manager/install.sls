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

/opt/apps/rtmp-proxy/shared:
  file.directory:
    - user: root
    - group: root
    - mode: '0700'
    - makedirs: true

stop-rtmp-proxy-before-upgrade:
  service.dead:
    - name: rtmp-proxy.service
    - onlyif: systemctl is-active --quiet rtmp-proxy.service
    - prereq:
      - archive: /opt/apps/rtmp-proxy/current

/opt/apps/rtmp-proxy/current:
  archive.extracted:
    - source: {{ pillar['rtmp_proxy']['release_url'] }}
    - source_hash: {{ pillar['rtmp_proxy']['release_url'] }}.sha256
    - source_hash_update: true
    - archive_format: tar
    - overwrite: true
    - user: root
    - group: root
    - enforce_ownership_on: /opt/apps/rtmp-proxy/current
