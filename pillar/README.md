# Encrypted pillar

Copy `rtmp-manager.sls.example` to `rtmp-manager.sls`, replace each placeholder
with an ASCII-armored GPG message encrypted to the infrastructure deployment
public key, and commit the encrypted file.

The private key belongs only on the deployment host. `salt-public-key.asc` may
be committed here after the infrastructure key is generated.

The deployment deliberately fails while `rtmp-manager.sls` is absent or cannot
be decrypted.
