{% set ffmpeg = pillar['rtmp_proxy']['ffmpeg'] %}
{% set release_url = pillar['rtmp_proxy']['release_url'] %}
{% set release_archive = '/opt/apps/rtmp-proxy/' ~ release_url.split('/')[-1] %}
{% set ffmpeg_archive = '/opt/ffmpeg/' ~ ffmpeg['source_url'].split('/')[-1] %}

{{ ffmpeg_archive }}:
  file.managed:
    - source: {{ ffmpeg['source_url'] }}
    - source_hash: {{ ffmpeg['source_hash'] }}
    - user: root
    - group: root
    - mode: '0644'
    - makedirs: true

{{ ffmpeg['install_dir'] }}:
  archive.extracted:
    - source: {{ ffmpeg_archive }}
    - overwrite: true
    - archive_format: tar
    - user: root
    - group: root
    - enforce_ownership_on: {{ ffmpeg['install_dir'] }}
    - onchanges:
      - file: {{ ffmpeg_archive }}

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
      - file: {{ release_archive }}

{{ release_archive }}:
  file.managed:
    - source: {{ release_url }}
    - source_hash: {{ release_url }}.sha256
    - user: root
    - group: root
    - mode: '0644'
    - makedirs: true

/opt/apps/rtmp-proxy/current:
  archive.extracted:
    - source: {{ release_archive }}
    - overwrite: true
    - archive_format: tar
    - user: root
    - group: root
    - enforce_ownership_on: /opt/apps/rtmp-proxy/current
    - onchanges:
      - file: {{ release_archive }}
