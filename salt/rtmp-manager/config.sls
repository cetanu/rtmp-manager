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

/etc/caddy/apps/rtmp-proxy.caddy:
  file.managed:
    - source: salt://rtmp-manager/files/rtmp-proxy.caddy
    - template: jinja2
    - user: root
    - group: root
    - mode: '0644'
