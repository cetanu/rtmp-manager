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
