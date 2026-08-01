{% set release_url = "https://github.com/cetanu/rtmp-manager/releases/latest/download/rtmp-proxy" %}

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
    - source: {{ release_url }}
    - source_hash: {{ release_url }}.sha256
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
