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
