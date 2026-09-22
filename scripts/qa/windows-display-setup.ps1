# Only a disposable GitHub-hosted Windows desktop may install this QA driver.
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'Use a disposable GitHub-hosted Windows runner'
}
New-Item -ItemType Directory -Force windows-display-review | Out-Null
$qa = Join-Path $env:RUNNER_TEMP 'kiri-display-driver'
New-Item -ItemType Directory -Force $qa | Out-Null
function Get-VerifiedArchive($url, $name, $sha) {
    $path = Join-Path $qa $name
    Invoke-WebRequest $url -OutFile $path
    if ((Get-FileHash $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $sha) {
        throw "Archive checksum mismatch: $name"
    }
    Expand-Archive $path -DestinationPath (Join-Path $qa $name.Replace('.zip', ''))
}
Get-VerifiedArchive 'https://github.com/VirtualDrivers/Virtual-Display-Driver/releases/download/25.7.23/VirtualDisplayDriver-x86.Driver.Only.zip' 'driver.zip' 'e24210692b442b39af763536330ce78b423f19342b7a7792c26de3944e418b3a'
Get-VerifiedArchive 'https://github.com/nefarius/nefcon/releases/download/v1.14.0/nefcon_v1.14.0.zip' 'nefcon.zip' 'a15557da24a9efca203158de3b43b0eaf982db231f0194031f1ed428bc13e669'
$driver = Join-Path $qa 'driver/VirtualDisplayDriver'
$signature = Get-AuthenticodeSignature (Join-Path $driver 'mttvdd.cat')
$signature | Format-List Status, StatusMessage, SignerCertificate | Out-File windows-display-review/driver-signature.txt
if ($signature.Status -ne 'Valid') { throw 'Virtual display driver catalog signature is not valid' }
# A valid Authenticode driver publisher still requires explicit installation
# consent. Trust only this verified leaf publisher on the disposable runner;
# never install a root certificate or disable Windows signature enforcement.
$publisher = Join-Path $qa 'publisher.cer'
[IO.File]::WriteAllBytes($publisher, $signature.SignerCertificate.Export([Security.Cryptography.X509Certificates.X509ContentType]::Cert))
Import-Certificate -FilePath $publisher -CertStoreLocation Cert:/LocalMachine/TrustedPublisher | Out-Null
# The driver reads this configuration from its documented default directory.
New-Item -ItemType Directory -Force C:/VirtualDisplayDriver | Out-Null
$settings = [xml](Get-Content (Join-Path $driver 'vdd_settings.xml'))
$settings.vdd_settings.monitors.count = '2'
$settings.Save('C:/VirtualDisplayDriver/vdd_settings.xml')
& (Join-Path $qa 'nefcon/x64/nefconc.exe') install (Join-Path $driver 'MttVDD.inf') 'Root\MttVDD' 2>&1 | Tee-Object windows-display-review/install.txt
if ($LASTEXITCODE -ne 0) { throw "Virtual display installation failed: $LASTEXITCODE" }
Start-Sleep -Seconds 8
& DisplaySwitch.exe /extend
Start-Sleep -Seconds 3
