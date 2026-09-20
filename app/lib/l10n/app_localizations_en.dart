// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String get appName => 'VaultSync';

  @override
  String get decoyCancel => 'Cancel';

  @override
  String get decoyClose => 'Close';

  @override
  String get decoyCreate => 'Create decoy space';

  @override
  String decoyCreateFailed(Object e) {
    return 'Could not create: $e';
  }

  @override
  String get decoyCreating => 'Creating…';

  @override
  String get decoyHintMissing =>
      'No decoy space yet: turning the switch on asks for a decoy password and creates one (separate file, separate MK).';

  @override
  String get decoyHintReady =>
      'Decoy space ready: the decoy password opens it on the lock screen (turning the switch off stops accepting that password).';

  @override
  String get decoyMismatch => 'The decoy passwords do not match';

  @override
  String get decoyNote =>
      'The decoy space is a separate encrypted container, so it accepts files normally as cover. The real vault refuses every call made from the decoy space, and which space you entered is never recorded (only that the padlock door was entered).';

  @override
  String get decoyPwConfirmLabel => 'Confirm decoy password';

  @override
  String get decoyPwHint =>
      'Must differ from the master password · used under duress';

  @override
  String get decoyPwLabel => 'Decoy password';

  @override
  String get decoySub =>
      'Separate vault file · separate MK · zero sharing with the real space';

  @override
  String get decoyTitleNew => 'Set up decoy entry';

  @override
  String get decoyTitleReset => 'Reset decoy password';

  @override
  String get devicesPageAddrHint => 'host:port, e.g. 192.168.1.10:9100';

  @override
  String get devicesPageBadgeLocal => 'This device';

  @override
  String get devicesPageBadgePaired => 'Paired';

  @override
  String get devicesPageCancel => 'Cancel';

  @override
  String get devicesPageCancelDestroy => 'Cancel wipe';

  @override
  String get devicesPageConfirmUnpair => 'Confirm unbind';

  @override
  String get devicesPageConnectPair => 'Connect and pair';

  @override
  String get devicesPageDelay10s => '10 s';

  @override
  String get devicesPageDelay10sSub => 'Leaves a window to undo a mistake';

  @override
  String get devicesPageDelay5m => '5 minutes';

  @override
  String get devicesPageDelay5mSub =>
      'Long delay · can be cancelled at any time';

  @override
  String get devicesPageDelay60s => '60 s';

  @override
  String get devicesPageDelay60sSub =>
      'Can be cancelled with \"Cancel wipe\" on the Devices page';

  @override
  String get devicesPageDelayImmediate => 'Immediately';

  @override
  String get devicesPageDelayImmediateSub =>
      'The command enters the signalling queue as soon as it is issued';

  @override
  String get devicesPageDestroyArmed => 'Remote wipe armed';

  @override
  String devicesPageDestroyArmedSub(String target, int seconds) {
    return 'Target $target · ${seconds}s remaining';
  }

  @override
  String devicesPageDestroyConfirmError(String word) {
    return 'Type the confirmation word \"$word\" exactly';
  }

  @override
  String devicesPageDestroyConfirmInput(String word) {
    return 'Type \"$word\" to confirm';
  }

  @override
  String get devicesPageDestroyConfirmWord => 'destroy';

  @override
  String get devicesPageDestroyDelay => 'Wipe delay';

  @override
  String get devicesPageDestroyNote =>
      'A wipe command signed with this device\'s key is issued to the target (high-priority signalling queue). If the device is offline, it runs the next time it comes online — before any other message is handled.';

  @override
  String get devicesPageDestroySendFailed =>
      'Could not send the remote wipe command';

  @override
  String get devicesPageDestroySent => 'Wipe command sent';

  @override
  String devicesPageDestroySentTo(String name) {
    return 'Remote wipe command sent to $name';
  }

  @override
  String devicesPageDestroyTarget(String name) {
    return 'Target: $name';
  }

  @override
  String get devicesPageDone => 'Done';

  @override
  String get devicesPageEmptySub =>
      'Unlock the vault first, or use \"Pair a new device\" to set up your first encrypted channel.';

  @override
  String get devicesPageEmptyTitle => 'No device data yet';

  @override
  String get devicesPageFingerprint => 'Fingerprint';

  @override
  String get devicesPageFingerprintConfirm => 'Fingerprints match — pair';

  @override
  String get devicesPageGenerateInvite => 'Generate invite code';

  @override
  String get devicesPageHeaderSub =>
      'Sync with paired devices over an end-to-end encrypted channel';

  @override
  String devicesPageHeaderSubCounts(int peers, int online) {
    return '$peers paired devices · $online online';
  }

  @override
  String get devicesPageInviteCode => 'One-time invite code';

  @override
  String get devicesPageInviteCodePasteHint =>
      'Paste the invite code from the other device';

  @override
  String get devicesPageInviteCodeValid =>
      'One-time invite code (valid for 10 minutes)';

  @override
  String get devicesPageInviteFailed => 'Could not generate the invite code';

  @override
  String get devicesPageInviteNote =>
      'The invite code is valid for 10 minutes and expires once used; it carries no sensitive information.';

  @override
  String get devicesPageIssueDestroy => 'Issue wipe command';

  @override
  String get devicesPageKvDevice => 'Device';

  @override
  String get devicesPageLastSyncJustNow => 'Last synced just now';

  @override
  String get devicesPageLocalFingerprint => 'This device\'s fingerprint';

  @override
  String get devicesPageLocalFingerprintShort => 'Local fingerprint';

  @override
  String get devicesPageManualAdd => 'Add manually';

  @override
  String get devicesPageManualHint =>
      'Enter the peer address and the one-time invite code generated by the other device:';

  @override
  String get devicesPageManualMode => 'Enter manually';

  @override
  String get devicesPageMitmNote =>
      'Mismatched fingerprints mean the channel may have been hijacked (MITM) — refuse to pair.';

  @override
  String get devicesPageNeedUnlock => 'Unlock the vault first';

  @override
  String get devicesPageNoPeersSub =>
      'Use \"Pair a new device\" in the top right to scan a QR code or enter an invite code and establish the channel.';

  @override
  String get devicesPageNoPeersTitle => 'No paired devices yet';

  @override
  String devicesPageNotifDestroyCancelFailed(int code) {
    return 'Could not cancel the wipe (error code $code)';
  }

  @override
  String get devicesPageNotifDestroyCancelled => 'Remote wipe cancelled';

  @override
  String get devicesPageNotifNeedUnlock => 'Unlock the vault first';

  @override
  String get devicesPageNotifPaired => 'Paired · added to the node list';

  @override
  String get devicesPageOffline => 'Offline';

  @override
  String get devicesPageOnline => 'Online';

  @override
  String get devicesPagePairDone => 'Pairing complete';

  @override
  String get devicesPagePairDoneSub =>
      'A long-term encrypted channel is in place · both node lists were updated · recorded in the audit log';

  @override
  String devicesPagePairFailed(String e) {
    return 'Pairing error: $e';
  }

  @override
  String get devicesPagePairFailedInvalid =>
      'Pairing failed: invalid address or invite code';

  @override
  String get devicesPagePairNew => 'Pair a new device';

  @override
  String get devicesPagePeerAddr => 'Peer device address';

  @override
  String get devicesPagePeerDevice => 'Peer device';

  @override
  String devicesPagePeerDeviceName(String name) {
    return 'Peer device: $name';
  }

  @override
  String get devicesPageQrHint =>
      'Use \"Generate invite code\" below to create a one-time invite code and QR code, which the other device connects with under Pair → Enter manually.';

  @override
  String get devicesPageRefresh => 'Refresh';

  @override
  String get devicesPageRemoteWipe => 'Remote wipe';

  @override
  String get devicesPageSelfRole => 'This device · primary node';

  @override
  String devicesPageSelfSub(String id, String port) {
    return '$id · port $port';
  }

  @override
  String get devicesPageSending => 'Sending…';

  @override
  String get devicesPageStepChoose => 'Choose a method';

  @override
  String get devicesPageStepDone => 'Pairing done';

  @override
  String get devicesPageStepVerify => 'Verify fingerprints';

  @override
  String get devicesPageThisComputer => 'This computer';

  @override
  String get devicesPageTitle => 'Devices';

  @override
  String get devicesPageUnbind => 'Unbind';

  @override
  String devicesPageUnpairFailed(String err) {
    return 'Could not unbind: $err';
  }

  @override
  String get devicesPageUnpairNote =>
      'Unpairing removes only this device\'s record of the peer: the other device and its data are unaffected, and no local file is deleted. Re-establishing the channel means exchanging a new one-time invite code and verifying fingerprints again.';

  @override
  String devicesPageUnpairSub(String name) {
    return 'Remove the trust record for \"$name\"';
  }

  @override
  String get devicesPageUnpairTitle => 'Unpair';

  @override
  String devicesPageUnpaired(String name) {
    return 'Unpaired from \"$name\"';
  }

  @override
  String get devicesPageVerifyHint =>
      'Compare these fingerprints (public-key hashes) on both devices:';

  @override
  String get devicesPageWizardSub =>
      'Exchange public keys · verify fingerprints · confirm on both sides';

  @override
  String get importDialogCancel => 'Cancel';

  @override
  String get importDialogDropHere => 'Drop files here';

  @override
  String get importDialogHint => 'Multi-select · any file type';

  @override
  String get importDialogImport => 'Import';

  @override
  String importDialogImportCount(int n) {
    return 'Import $n files';
  }

  @override
  String get importDialogNoFiles => 'No files selected yet';

  @override
  String get importDialogNote =>
      'Ciphertext thumbnails and the full-text index are built right after import (plaintext never touches disk).';

  @override
  String get importDialogOrClick => ', or click to choose';

  @override
  String get importDialogRemove => 'Remove';

  @override
  String get importDialogSub =>
      'Streamed reads · CDC chunking · AES-256-GCM per chunk';

  @override
  String get importDialogTitle => 'Import files';

  @override
  String get lockScreenBindManage => 'Bind / manage';

  @override
  String get lockScreenBiometric => 'Biometric unlock';

  @override
  String get lockScreenBusy => 'Working…';

  @override
  String lockScreenCooldown(Object remaining) {
    return 'Too many attempts — brute-force protection engaged · retry in $remaining';
  }

  @override
  String get lockScreenCreateAndUnlock => 'Create and unlock';

  @override
  String lockScreenFirstRunNote(Object n) {
    return 'First launch · master password ≥$n characters with mixed character types; the MK is generated by the CSPRNG, so changing the password only re-wraps it';
  }

  @override
  String get lockScreenForgotPassword => 'Forgot master password?';

  @override
  String lockScreenHintWithAttempts(Object hint, Object n) {
    return '$hint (attempt $n)';
  }

  @override
  String get lockScreenPwEnterHint => 'Enter master password';

  @override
  String lockScreenPwSetHint(Object n) {
    return 'Set a master password (at least $n characters)';
  }

  @override
  String lockScreenPwStrengthOk(Object label) {
    return '$label · acceptable (KEK derived with Argon2id)';
  }

  @override
  String lockScreenPwStrengthWeak(Object label, Object n) {
    return '$label · needs at least $n characters and mixed character types';
  }

  @override
  String get lockScreenPwSuggestMix =>
      'Mix upper and lower case and add a digit or symbol';

  @override
  String lockScreenPwTooShort(Object n) {
    return 'At least $n characters';
  }

  @override
  String lockScreenPwWrongAttempts(Object n) {
    return 'Wrong master password (attempt $n)';
  }

  @override
  String get lockScreenRecoveryInfo =>
      'The MK is generated by the CSPRNG and cannot be derived back from your password, so it cannot be recovered from memory. Resetting requires authorization from a paired device (P5), or use the disguise entrance to enter the decoy space.';

  @override
  String get lockScreenTagline => 'YOUR DATA · EVERYWHERE · ENCRYPTED';

  @override
  String get lockScreenTitle => 'VaultSync Vault';

  @override
  String get lockScreenTogglePwVisibility => 'Show / hide master password';

  @override
  String get lockScreenUnlock => 'Unlock';

  @override
  String get navDevices => 'Devices';

  @override
  String get navSecurity => 'Security';

  @override
  String get navSettings => 'Settings';

  @override
  String get navStego => 'Stego';

  @override
  String get navSync => 'Sync';

  @override
  String get navVault => 'Vault';

  @override
  String get securityAuditChainBroken => 'Chain check failed';

  @override
  String get securityAuditChainOk => 'Chain verified';

  @override
  String get securityAuditEmpty =>
      'No audit records yet · unlock/import/sync operations are recorded here';

  @override
  String get securityAuditExport => 'Export';

  @override
  String securityAuditExportFailed(String err) {
    return 'Audit export failed: $err';
  }

  @override
  String securityAuditExportedTo(String path) {
    return 'Audit log encrypted and exported to $path';
  }

  @override
  String securityAuditHead(String short) {
    return 'Local chain head $short';
  }

  @override
  String get securityAuditLoadFailed =>
      'Failed to read the audit log (engine unavailable or session expired)';

  @override
  String get securityAuditNote =>
      'The audit chain is per device (one chain each, docs/05-06 §3.1); this page shows the local chain only. Failed attempts while locked cannot be written (the log key is derived from the MK), so the chain holds successful operations only. The log is sealed as a whole with AEAD, so the first load of a long log takes a moment.';

  @override
  String get securityAuditTitle => 'Audit log';

  @override
  String get securityAuditVerify => 'Verify chain';

  @override
  String securityAuditVerifyFailed(int at, String reason) {
    return 'Audit chain verification failed from entry $at: $reason';
  }

  @override
  String securityAuditVerifyOk(int checked) {
    return 'Audit chain verified ($checked entries)';
  }

  @override
  String get securityAuditVerifyUnavailable =>
      'Audit chain unreadable; verification was not run';

  @override
  String get securityBadgeGuarantee => 'By design';

  @override
  String get securityCatDevice => 'Device';

  @override
  String get securityCatSecurity => 'Security';

  @override
  String get securityCatSession => 'Session';

  @override
  String get securityCatVault => 'Vault';

  @override
  String get securityColAction => 'Action / object';

  @override
  String get securityColCategory => 'Category';

  @override
  String get securityColHash => 'Chain hash';

  @override
  String get securityColSeq => 'Seq';

  @override
  String get securityColTime => 'Time';

  @override
  String securityDestroyAllConfirmBody(int n) {
    return 'The engine has no offline command queue, so a destroy command cannot be delivered to offline devices. Confirming wipes this device only; the $n paired device(s) must each be wiped from their own device page while online.';
  }

  @override
  String get securityDestroyAllConfirmTitle => 'Confirm fleet-wide wipe';

  @override
  String get securityDestroyCancel => 'Cancel';

  @override
  String get securityDestroyConfirm => 'Confirm';

  @override
  String securityDestroyConfirmInput(String word) {
    return 'Type \"$word\" to confirm';
  }

  @override
  String get securityDestroyDialogSub =>
      'Irreversible · confirmation phrase required';

  @override
  String get securityDestroyDialogTitle => 'Emergency wipe';

  @override
  String get securityDestroyDone => 'Local data cryptographically erased';

  @override
  String get securityDestroyEnter => 'Proceed to wipe';

  @override
  String get securityDestroyExportCancelled =>
      'No export path selected; wipe cancelled';

  @override
  String get securityDestroyExportFirst => 'Export the audit log first';

  @override
  String get securityDestroyExportFirstSub =>
      'The data directory is wiped together with the audit log; export it to a chosen path first';

  @override
  String securityDestroyExported(String path) {
    return 'Audit log exported to $path';
  }

  @override
  String securityDestroyFailed(String err) {
    return 'Wipe failed: $err';
  }

  @override
  String get securityDestroyModeAll => 'Fleet-wide wipe';

  @override
  String get securityDestroyModeAllSub =>
      'Signs a destroy command and propagates it to every paired device over P2P / relay';

  @override
  String get securityDestroyModeLocal => 'Wipe this device';

  @override
  String get securityDestroyModeLocalSub =>
      'Destroys the local MK and every key slot, with media fallback overwrite';

  @override
  String get securityDestroyNote =>
      'Offline devices do not receive the destroy command immediately: it enters each device\'s high-priority signalling queue, and the target verifies and executes it before any other message once it comes online (signed by the initiating device key, so it cannot be forged). For a device that never comes online, the wipe cannot be guaranteed.';

  @override
  String get securityDestroyNow => 'Wipe now';

  @override
  String get securityDestroyPeersHint =>
      'Bring the peer online first, then start the wipe from that device\'s page';

  @override
  String get securityDestroyPeersNone =>
      'Fleet-wide wipe: no paired devices, only this device was erased';

  @override
  String securityDestroyPeersUndelivered(int n) {
    return 'Fleet-wide wipe not delivered: this device is erased, $n paired device(s) must be wiped from their own device page while online';
  }

  @override
  String get securityDestroyPhrase => 'destroy';

  @override
  String securityDestroyQueued(int n) {
    return 'Destroy orders signed and queued for $n paired device(s); each executes before any other message the next time it comes online.';
  }

  @override
  String get securityDestroyRunning => 'Wiping…';

  @override
  String get securityDestroySub =>
      'Local / remote / fleet-wide · cryptographic erase';

  @override
  String get securityDestroyTitle => 'Emergency wipe';

  @override
  String get securityDetectAudit => 'Audit chain integrity';

  @override
  String securityDetectAuditBroken(int at, String reason) {
    return 'Verification failed from entry $at: $reason';
  }

  @override
  String securityDetectAuditOk(int checked, int total) {
    return 'Normal · $checked / $total entries verified';
  }

  @override
  String get securityDetectAuditUnknown =>
      'Audit chain unreadable · cannot verify';

  @override
  String get securityDetectBrute => 'Brute-force detection';

  @override
  String securityDetectBruteCooldown(int m, int s) {
    return 'Cooling down · $m min $s s remaining';
  }

  @override
  String get securityDetectBruteOk => 'Normal · no active cooldown';

  @override
  String get securityDetectDevice => 'Unexpected devices';

  @override
  String get securityDetectDeviceNone => 'No paired devices yet';

  @override
  String securityDetectDeviceOk(int n) {
    return '$n device(s) paired · no unauthorized access recorded';
  }

  @override
  String securityDetectDeviceWarn(String event) {
    return 'Attention · $event';
  }

  @override
  String get securityDetectIntegrity => 'File integrity';

  @override
  String securityDetectIntegrityFailed(int failed, int files) {
    return 'Alert · $failed / $files containers failed verification';
  }

  @override
  String get securityDetectIntegrityFailedRead =>
      'Integrity scan failed: the vault could not be read';

  @override
  String get securityDetectIntegrityNotScanned =>
      'Not scanned yet · click the card to re-verify every chunk';

  @override
  String securityDetectIntegrityOk(int checked, int files) {
    return 'OK · $checked / $files containers verified chunk by chunk';
  }

  @override
  String get securityDetectIntegrityScanning =>
      'Scanning… (per-chunk re-hash; time grows with data size)';

  @override
  String get securityDetectIntegritySub =>
      'Per-chunk GCM authentication on every export and sync · rejected on failure';

  @override
  String get securityDetectMemory => 'Memory leak scan';

  @override
  String get securityDetectMemorySub =>
      'Keys and plaintext buffers are Zeroizing end to end · the MK never leaves the engine';

  @override
  String get securityDetectSignal => 'Anti-replay signalling';

  @override
  String get securityDetectSignalDown =>
      'Engine stopped · signal sequence continuity unverified';

  @override
  String securityDetectSignalOk(int n) {
    return 'Noise channel + monotonic application sequence numbers (docs/08 §4.1) · local engine alive · $n signalling event(s)';
  }

  @override
  String get securityDetectSignalUnknown =>
      'P2P status unavailable · cannot determine';

  @override
  String get securityFilterAll => 'All';

  @override
  String get securityNeedUnlock => 'Unlock the vault first';

  @override
  String get securityOverallDanger => 'Overall: critical';

  @override
  String get securityOverallOk => 'Overall: normal';

  @override
  String get securityOverallWarn => 'Overall: attention';

  @override
  String get securityRefresh => 'Refresh';

  @override
  String get securityRekeyDo => 'Rotate now';

  @override
  String get securityRekeyDone =>
      'Master key rotated · unlock again with your master password (existing shares invalidated)';

  @override
  String securityRekeyFailed(String err) {
    return 'Rotation failed: $err';
  }

  @override
  String get securityRekeyNote =>
      'Rotation rewrites the index and all containers (time grows with data size) and re-wraps KEK_pwd under a fresh salt (a bound biometric copy is re-wrapped too). The vector clock is not advanced, so it does not propagate to paired devices; existing burn-after-reading share tokens are invalidated. You must unlock again with your master password afterwards.';

  @override
  String get securityRekeyNow => 'Rotate MK';

  @override
  String get securityRekeyPhrase => 'Type “rotate” to confirm';

  @override
  String get securityRekeyPhraseWord => 'rotate';

  @override
  String get securityRekeyPw => 'Master password';

  @override
  String get securityRekeySub =>
      'Emergency path when the MK leaks: rewrite the index and every container under a new MK';

  @override
  String get securityRekeyTitle => 'Rotate master key';

  @override
  String get securityRekeyWrongPw => 'Wrong master password';

  @override
  String get securityStegoDisable => 'Disable';

  @override
  String get securityStegoDisabled => 'Steganography engine disabled';

  @override
  String get securityStegoEnable => 'Enable';

  @override
  String get securityStegoEnabled => 'Steganography engine enabled';

  @override
  String securityStegoFailed(String err) {
    return 'Failed to change the steganography engine: $err';
  }

  @override
  String get securityStegoSubOff =>
      'Hidden by default · covert storage in image LSBs · enable/disable is audited';

  @override
  String get securityStegoSubOn =>
      'Enabled · covert storage in image LSBs · enable/disable is audited';

  @override
  String get securityStegoTitle => 'Steganography engine';

  @override
  String get securitySub => 'Audit · Detection · Incident response';

  @override
  String get securityTitle => 'Security Center';

  @override
  String get settingsAbout => 'About';

  @override
  String get settingsAboutSub => 'Version and design docs';

  @override
  String get settingsAppearance => 'Appearance & language';

  @override
  String get settingsAppearanceSub => 'Theme · Language';

  @override
  String get settingsAutoLockSub => 'Locks the vault after an idle timeout';

  @override
  String get settingsAutoLockTitle => 'Auto-lock';

  @override
  String get settingsAutoSync => 'Enable auto-sync';

  @override
  String get settingsAutoSyncSub =>
      'Selective sync: folders, size, network, devices (refined in P5)';

  @override
  String get settingsBioBound => 'Bound · Windows Hello fingerprint';

  @override
  String get settingsBioBoundDone => 'Biometrics bound · MK_wrap_bio appended';

  @override
  String get settingsBioHello => 'Windows Hello fingerprint';

  @override
  String get settingsBioHelloSub =>
      'The key is held by the platform secure enclave; this app never touches biometric data';

  @override
  String get settingsBioLoading => 'Reading…';

  @override
  String get settingsBioNote =>
      'After a secure enclave reset the biometric copy is invalidated automatically; the master password path is unaffected.';

  @override
  String get settingsBioTitle => 'Biometrics';

  @override
  String get settingsBioUnbound =>
      'Not bound · master password always available';

  @override
  String get settingsBioUnboundDone => 'Biometrics unbound';

  @override
  String get settingsCancel => 'Cancel';

  @override
  String get settingsChange => 'Change';

  @override
  String get settingsChangePw => 'Change master password';

  @override
  String get settingsChangePwDialogSub =>
      'Rewraps MK only · no data re-encryption';

  @override
  String get settingsChangePwDone =>
      'Master password changed · MK rewrapped, no data re-encryption';

  @override
  String get settingsChangePwNote =>
      'The change regenerates MK_wrap_pwd with a fresh salt; paired devices and ciphertext are unaffected.';

  @override
  String get settingsChangePwSub =>
      'Done in milliseconds, with no effect on encrypted files or sync';

  @override
  String get settingsConfirmChange => 'Confirm change';

  @override
  String get settingsConfirmPw => 'Confirm new master password';

  @override
  String get settingsCurrentPw => 'Current master password';

  @override
  String get settingsDecoyAllow => 'Allow decoy password';

  @override
  String get settingsDecoyAllowSub =>
      'The decoy password opens the decoy vault on the lock screen; real vault calls are always refused';

  @override
  String get settingsDecoySub =>
      'Enter a duress password to open a separate decoy vault';

  @override
  String get settingsDecoyTitle => 'Decoy entry';

  @override
  String get settingsDesignDocLabel => 'Design doc';

  @override
  String get settingsEngineLabel => 'Engine';

  @override
  String get settingsForgotPw => 'Forgot master password';

  @override
  String get settingsForgotPwSub =>
      'Secure reset flow, requires approval from a paired device (P5)';

  @override
  String get settingsIdleTimeout => 'Idle timeout';

  @override
  String get settingsIdleTimeoutSub => 'Lock after inactivity (0 = never)';

  @override
  String get settingsLanguage => 'Language';

  @override
  String get settingsLanguageEn => 'English';

  @override
  String get settingsLanguageSub => 'Interface language · applies immediately';

  @override
  String get settingsLanguageSystem => 'Follow system';

  @override
  String get settingsLanguageZh => '中文';

  @override
  String settingsMinutes(int n) {
    return '$n min';
  }

  @override
  String get settingsNeedUnlock => 'Unlock the vault first';

  @override
  String get settingsNever => 'Never';

  @override
  String get settingsNewPw => 'New master password';

  @override
  String get settingsOpenDocsFailed => 'Could not open the docs link';

  @override
  String get settingsPwMismatch => 'The new passwords do not match';

  @override
  String get settingsPwSection => 'Master password';

  @override
  String get settingsPwSectionSub =>
      'Changing the password only rewraps the master key; data is never re-encrypted';

  @override
  String settingsStrengthHint(String label) {
    return '$label · Argon2id-derived KEK';
  }

  @override
  String get settingsSub => 'Security · Lock · Sync · Appearance';

  @override
  String get settingsSubmitting => 'Submitting…';

  @override
  String get settingsSyncOff => 'Auto-sync is off';

  @override
  String get settingsSyncOn =>
      'Auto-sync is on · manual trigger once turned off';

  @override
  String get settingsSyncTitle => 'Sync policy';

  @override
  String get settingsTheme => 'Theme';

  @override
  String get settingsThemeDark => 'Dark (default)';

  @override
  String get settingsThemeLight => 'Light';

  @override
  String get settingsThemeSub =>
      'Dark is the default · matches the prototype\'s data-theme';

  @override
  String get settingsThemeSystem => 'Follow system';

  @override
  String get settingsTitle => 'Settings';

  @override
  String get settingsUpdateCheck => 'Check for updates';

  @override
  String get settingsUpdateChecking => 'Checking…';

  @override
  String get settingsUpdateDownload => 'Download and install';

  @override
  String get settingsUpdateNotConfigured =>
      'Not configured · needs a manifest URL and release key (fail-closed)';

  @override
  String get settingsUpdateNote =>
      'The signature covers the manifest\'s raw bytes and the installer is re-checked against the sha256 inside the manifest; any failure rejects the update. This repository ships no release server or signing key, so the channel stays disabled until configured.';

  @override
  String get settingsUpdatePubkey => 'Release public key';

  @override
  String get settingsUpdatePubkeySub =>
      'Pinned Ed25519 key (hex, 32 bytes) — only manifests signed by it are accepted';

  @override
  String get settingsUpdateTitle => 'Update channel';

  @override
  String get settingsUpdateUrl => 'Manifest URL';

  @override
  String get settingsUpdateUrlSub =>
      'Publisher-hosted latest.json (https only); latest.json.sig next to it holds the Ed25519 signature';

  @override
  String get settingsVersionLabel => 'Version';

  @override
  String settingsVersionValue(String version) {
    return 'VaultSync $version · end-to-end encrypted multi-device sync';
  }

  @override
  String get settingsViewDocs => 'Open online docs';

  @override
  String get shellAllFiles => 'All files';

  @override
  String get shellLockNow => 'Lock now';

  @override
  String get shellMarkAllRead => 'Mark all read';

  @override
  String get shellNavSectionControl => 'Control';

  @override
  String get shellNavSectionSpace => 'Space';

  @override
  String get shellNoNotifications => 'No notifications';

  @override
  String get shellNotifications => 'Notifications';

  @override
  String get shellPlaceholderSub => 'In development — see docs/09-开发任务面板.md';

  @override
  String get shellSearchHint => 'Search files, tags, full text…';

  @override
  String get shellSubDevices =>
      'Sync with paired devices over an end-to-end encrypted channel';

  @override
  String get shellSubSecurity => 'Audit · detection · incident response';

  @override
  String get shellSubSettings => 'Security · auto-lock · sync · appearance';

  @override
  String get shellSubStego => 'AES-256-GCM ciphertext embedded in image LSBs';

  @override
  String get shellSubSync =>
      'Incremental sync · only differing chunks are transferred';

  @override
  String get shellTitleDevices => 'Devices';

  @override
  String get shellTitleSecurity => 'Security';

  @override
  String get shellTitleSettings => 'Settings';

  @override
  String get shellTitleStego => 'Steganography';

  @override
  String get shellTitleSync => 'Sync';

  @override
  String get shellTitleVault => 'Vault';

  @override
  String get shellVersionTagline => 'V0.2 · Desktop';

  @override
  String get stateBioFailed => 'Biometric unlock failed';

  @override
  String get stateBioNotBound =>
      'Biometrics are not bound yet (bind them in Settings)';

  @override
  String get stateBioUnsupported =>
      'Biometrics are unavailable on this platform — use the master password';

  @override
  String get stateConflictDropped => 'Discarded the conflict copy';

  @override
  String stateConflictKept(Object name) {
    return 'Kept the copy and replaced the original · $name';
  }

  @override
  String stateConflictResolveFailed(Object err) {
    return 'Could not resolve the conflict: $err';
  }

  @override
  String get stateCooldownHint =>
      'Too many attempts — wait for the cooldown to end';

  @override
  String stateCooldownSeconds(Object n) {
    return 'Too many attempts — retry in ${n}s';
  }

  @override
  String stateCreateFailed(Object e) {
    return 'Could not create the vault: $e';
  }

  @override
  String get stateDeviceLoadFailed => 'Could not read the device status';

  @override
  String get stateDeviceNeedUnlock => 'Unlock the vault first';

  @override
  String get stateNeedUnlock => 'Unlock the vault first';

  @override
  String get statePwMin4 => 'Password must be at least 4 characters';

  @override
  String get statePwRequired => 'Enter your master password';

  @override
  String statePwTooShort(Object n) {
    return 'Master password must be at least $n characters';
  }

  @override
  String get statePwTooWeak =>
      'Too weak: mix upper and lower case and add digits or symbols';

  @override
  String get statePwWrong => 'Wrong password';

  @override
  String get stateStrengthFair => 'Fair';

  @override
  String get stateStrengthGood => 'Good';

  @override
  String get stateStrengthStrong => 'Strong';

  @override
  String get stateStrengthWeak => 'Weak';

  @override
  String stateSyncDone(Object pulled, Object pushed) {
    return 'Sync complete: pulled $pulled, pushed $pushed';
  }

  @override
  String stateSyncDoneWithConflicts(
    Object conflicts,
    Object pulled,
    Object pushed,
  ) {
    return 'Sync complete: pulled $pulled, pushed $pushed, conflicts $conflicts';
  }

  @override
  String get stateSyncFailed => 'Sync failed';

  @override
  String get stateSyncRelayFailed => 'Relay sync failed';

  @override
  String get stateUnknownDevice => 'Unknown device';

  @override
  String get stegoPageCancel => 'Cancel';

  @override
  String get stegoPageCapacityCard => 'Capacity reference';

  @override
  String get stegoPageCapacityFormat =>
      'Unsupported image: the stego engine accepts 8-bit RGB / RGBA PNG only (lossy compression destroys LSBs)';

  @override
  String get stegoPageCapacityFormula =>
      'Capacity = width × height × 3 ÷ 8 − 4 (the 4 bytes are the payload length prefix)';

  @override
  String get stegoPageCapacityIo =>
      'Could not read the image: the file is missing or unreadable';

  @override
  String get stegoPageCapacityLimit =>
      'One image holds little: larger files must be split across several images (docs/05-05 §3.2).';

  @override
  String stegoPageCapacityLine(String cap, String need) {
    return 'Capacity $cap / needed $need';
  }

  @override
  String stegoPageCapacityShort(String need) {
    return 'Not enough capacity: $need short — use a larger image (or split across several images).';
  }

  @override
  String get stegoPageDisable => 'Disable';

  @override
  String get stegoPageDisableDone => 'Stego engine disabled';

  @override
  String get stegoPageDisabled => 'Disabled';

  @override
  String get stegoPageDisabledNote =>
      'The stego engine is off by default: embed / extract are only accepted once it is enabled, and both enabling and disabling are written to the audit chain (docs/05-06 §7).';

  @override
  String get stegoPageEmbedAction => 'Embed';

  @override
  String stegoPageEmbedDone(String path) {
    return 'Embedded → $path';
  }

  @override
  String get stegoPageEmbedNote =>
      'Payload = AEAD ciphertext of [length prefix ‖ file name ‖ plaintext]: the 4-byte length prefix and the AEAD overhead (12-byte nonce + 16-byte tag) also count against capacity.';

  @override
  String get stegoPageEmbedSub =>
      'Vault file → AES-256-GCM ciphertext → PNG LSB';

  @override
  String get stegoPageEmbedTitle => 'Stego import (embed)';

  @override
  String get stegoPageEmbedding => 'Embedding…';

  @override
  String get stegoPageEnable => 'Enable stego engine';

  @override
  String get stegoPageEnableDone => 'Stego engine enabled';

  @override
  String get stegoPageEnabled => 'Enabled';

  @override
  String get stegoPageEnabledNote =>
      'The payload is encrypted first, then embedded: the image LSBs hold nothing but AES-256-GCM ciphertext under a key derived from this vault\'s MK via HKDF — extraction needs that same master key, so this is a local concealment aid, not a cross-device channel.';

  @override
  String get stegoPageExtractAction => 'Extract';

  @override
  String stegoPageExtractDone(String name, String size) {
    return 'Restored $name ($size)';
  }

  @override
  String get stegoPageExtractFailed =>
      'Extraction failed: the image holds no valid payload, is not an 8-bit RGB/RGBA PNG, or the payload belongs to another vault (master key mismatch).';

  @override
  String get stegoPageExtractNote =>
      'Extraction only works inside the same vault: the payload key is derived from this machine\'s MK, so another vault or device cannot decrypt it.';

  @override
  String get stegoPageExtractSub =>
      'PNG LSB → ciphertext frame → AES-256-GCM decrypt';

  @override
  String get stegoPageExtractTitle => 'Stego extract (retrieve)';

  @override
  String get stegoPageExtracting => 'Extracting…';

  @override
  String get stegoPageFileEmpty => 'No files in the vault (import one first)';

  @override
  String get stegoPageFileLoading => 'Reading vault files…';

  @override
  String stegoPageFileOption(String name, String size, String folder) {
    return '$name · $size · $folder';
  }

  @override
  String get stegoPageFileRoot => 'Root';

  @override
  String get stegoPageNeedUnlock =>
      'Session is locked; stego operations are unavailable';

  @override
  String get stegoPageNotChosen => 'Not chosen';

  @override
  String get stegoPagePickFile => 'Vault file';

  @override
  String get stegoPagePickFileHint => 'Choose the file to embed';

  @override
  String get stegoPagePickImage => 'Choose PNG image';

  @override
  String get stegoPagePickOutput => 'Choose output location';

  @override
  String get stegoPagePickRestore => 'Choose restore location';

  @override
  String get stegoPagePickStegoImage => 'Choose PNG carrying data';

  @override
  String stegoPagePlainSize(String size) {
    return 'Plaintext size ≈ $size';
  }

  @override
  String get stegoPageSafetyNote =>
      'Steganography is a concealment aid, not a replacement for the main vault; lossy compression (JPG) destroys LSBs, so use PNG.';

  @override
  String stegoPageSizeB(int n) {
    return '$n B';
  }

  @override
  String stegoPageSizeBytes(int bytes, String size) {
    return '$bytes bytes ≈ $size';
  }

  @override
  String stegoPageSizeKb(String n) {
    return '$n KB';
  }

  @override
  String stegoPageSizeMb(String n) {
    return '$n MB';
  }

  @override
  String get stegoPageSub =>
      'Concealment on top of encryption · AES-256-GCM ciphertext in image LSBs';

  @override
  String get stegoPageTileEmbed => 'Stego import (embed)';

  @override
  String get stegoPageTileEmbedSub =>
      'Embed an already-encrypted vault file into an ordinary image; the image looks unchanged.';

  @override
  String get stegoPageTileExtract => 'Stego extract (retrieve)';

  @override
  String get stegoPageTileExtractSub =>
      'Pull the ciphertext frame out of an image and decrypt the original file.';

  @override
  String get stegoPageTileLocked => 'Enable the stego engine first';

  @override
  String get stegoPageTitle => 'Steganography Engine';

  @override
  String stegoPageToggleFailed(String err) {
    return 'Operation failed: $err';
  }

  @override
  String get syncPageAddrHint => 'host:port, e.g. 192.168.1.10:9100';

  @override
  String get syncPageCancel => 'Cancel';

  @override
  String get syncPageChunkDetails => 'Chunk details';

  @override
  String get syncPageChunkDetailsNote =>
      'The per-chunk transfer matrix (encrypted chunk = sync chunk = transport chunk) is wired to real data in P5.';

  @override
  String syncPageConflictBanner(int n) {
    return '$n conflicting files found — conflict copies are kept under .conflict-<timestamp> and can be handled in the vault list.';
  }

  @override
  String syncPageConflictCopyId(int id) {
    return 'Conflict copy #$id';
  }

  @override
  String get syncPageConflictNote =>
      'When the two sides diverge the engine keeps a conflict copy automatically (the local version is saved as <name>.conflict-<timestamp>). \"Keep the copy\" renames it back and replaces the peer version; \"Discard the copy\" deletes only the copy — both are written to the audit log.';

  @override
  String syncPageConflictPeerGone(String stamp) {
    return 'Copy kept at $stamp · the peer version is gone';
  }

  @override
  String get syncPageConflictSection => 'Conflict handling';

  @override
  String syncPageConflictStoredAt(String stamp) {
    return 'Copy kept at $stamp';
  }

  @override
  String get syncPageDialogNote =>
      'Sync is chunk-level and incremental: only chunks that differ are transferred, and chunks already present are reused after a strong-hash check.';

  @override
  String get syncPageDialogSub =>
      'Direct link or your own relay · Noise-encrypted channel';

  @override
  String get syncPageDirect => 'Direct';

  @override
  String get syncPageDropCopy => 'Discard the copy';

  @override
  String get syncPageEngineNote =>
      'The engine has no transfer queue or per-chunk progress callback yet: the stats, events and conflicts on this page are real data, and the chunk matrix will be wired in once the engine provides callbacks (P5).';

  @override
  String get syncPageEvents => 'Sync events';

  @override
  String syncPageEventsRecent(int n) {
    return 'Latest $n';
  }

  @override
  String get syncPageHeaderSub =>
      'Incremental sync · only differing chunks are transferred (CDC)';

  @override
  String get syncPageKeepCopy => 'Keep the copy';

  @override
  String get syncPageLatestSummary => 'Latest sync summary';

  @override
  String get syncPageLive => 'Live';

  @override
  String get syncPageNeverSynced => 'Not synced yet';

  @override
  String get syncPageNoConflicts => 'No conflict copies · both sides match';

  @override
  String get syncPageNoEvents =>
      'No events yet · they are recorded here after you run \"Sync now\"';

  @override
  String get syncPagePeerAddr => 'Peer address';

  @override
  String syncPagePendingCount(int n) {
    return '$n to resolve';
  }

  @override
  String get syncPageRefresh => 'Refresh';

  @override
  String get syncPageRelay => 'Relay';

  @override
  String get syncPageRelayAddr => 'Relay address';

  @override
  String get syncPageRoomHint => 'Shared room ID';

  @override
  String get syncPageRoomId => 'Room ID';

  @override
  String get syncPageScanning => 'Scanning…';

  @override
  String syncPageSelfPeers(String name, int peers) {
    return 'This device $name · $peers paired';
  }

  @override
  String get syncPageStartSync => 'Start sync';

  @override
  String get syncPageStatConflicts => 'Conflicts to resolve';

  @override
  String get syncPageStatDeleted => 'Items deleted';

  @override
  String get syncPageStatPulledFiles => 'Files pulled';

  @override
  String get syncPageStatPushedFiles => 'Files pushed';

  @override
  String get syncPageStatus => 'Sync status';

  @override
  String get syncPageStatusIdle => 'Idle · no transfer in progress';

  @override
  String get syncPageStatusRunning =>
      'Syncing · chunk-level transfer in progress (single blocking session)';

  @override
  String syncPageSummaryConflicts(int n) {
    return 'Conflicts $n';
  }

  @override
  String syncPageSummaryDeleted(int n) {
    return 'Deleted $n';
  }

  @override
  String syncPageSummaryLine(
    int pulled,
    int pushed,
    int deleted,
    int conflicts,
  ) {
    return 'Pulled $pulled · pushed $pushed · deleted $deleted · conflicts $conflicts';
  }

  @override
  String syncPageSummaryMerged(int n) {
    return 'Merged $n';
  }

  @override
  String syncPageSummaryPulled(int n) {
    return 'Pulled $n';
  }

  @override
  String syncPageSummaryPushed(int n) {
    return 'Pushed $n';
  }

  @override
  String get syncPageSyncNow => 'Sync now';

  @override
  String get syncPageSyncing => 'Syncing…';

  @override
  String get syncPageTitle => 'Sync';

  @override
  String get vaultPageAlgoLabel => 'Algorithm';

  @override
  String get vaultPageAlgoValue => 'AES-256-GCM · FastCDC 64KB';

  @override
  String get vaultPageBackToFolder => 'Back to current folder';

  @override
  String get vaultPageBurn => 'Burn after read';

  @override
  String get vaultPageCancel => 'Cancel';

  @override
  String get vaultPageCipherThumb => 'Ciphertext thumbnail';

  @override
  String get vaultPageClose => 'Close';

  @override
  String get vaultPageConflict => 'Conflict';

  @override
  String get vaultPageCreate => 'Create';

  @override
  String vaultPageCreatedFolder(String name) {
    return 'Created \"$name\" · subkey FSK derived';
  }

  @override
  String vaultPageDetailSub(String size, String kind) {
    return '$size · $kind';
  }

  @override
  String get vaultPageDetailThumb =>
      'Encrypted thumbnail · decrypted preview after unlock';

  @override
  String get vaultPageDone => 'Done';

  @override
  String get vaultPageEmptyFolderSub =>
      'Drop files here, or use the button below to import (chunked encryption is automatic).';

  @override
  String get vaultPageEmptyFolderTitle => 'This folder is empty';

  @override
  String get vaultPageEmptySearchSub =>
      'Try another keyword, or go back to the folder and search again.';

  @override
  String get vaultPageEmptySearchTitle => 'No matches';

  @override
  String get vaultPageEmptyVaultSub =>
      'Imported files are chunk-encrypted automatically; keys stay on this device only.';

  @override
  String get vaultPageEmptyVaultTitle => 'This vault is empty';

  @override
  String get vaultPageEncInfo => 'Encryption details';

  @override
  String get vaultPageExport => 'Export';

  @override
  String vaultPageExportCount(int n) {
    return 'Export $n files';
  }

  @override
  String vaultPageExportFailedItem(String name, String error) {
    return 'Export failed for $name: $error';
  }

  @override
  String get vaultPageExportNoneSelected =>
      'Nothing in the selection can be exported';

  @override
  String get vaultPageExportToHere => 'Export here';

  @override
  String vaultPageExportedMany(int done, int total, String dir) {
    return 'Exported $done/$total files to $dir';
  }

  @override
  String vaultPageExportedTo(String path) {
    return 'Exported to $path · final SHA-256 check passed';
  }

  @override
  String get vaultPageFileLabel => 'File';

  @override
  String get vaultPageFolder => 'Folder';

  @override
  String get vaultPageFsKeyLabel => 'File key';

  @override
  String get vaultPageFsKeyValue => 'FSKey · randomly generated';

  @override
  String get vaultPageImport => 'Import files';

  @override
  String vaultPageImportFailed(String name) {
    return 'Import failed: $name';
  }

  @override
  String vaultPageImportFileError(String name, String error) {
    return '$name: $error';
  }

  @override
  String vaultPageImportFolderUnsupported(String name) {
    return '$name (folders cannot be imported yet)';
  }

  @override
  String vaultPageImportedCount(int n) {
    return 'Imported $n files · CDC-chunked and encrypted into the vault';
  }

  @override
  String get vaultPageKindArchive => 'Archive';

  @override
  String get vaultPageKindDoc => 'Document';

  @override
  String get vaultPageKindFile => 'File';

  @override
  String get vaultPageKindFolder => 'Encrypted folder';

  @override
  String get vaultPageKindImage => 'Image';

  @override
  String get vaultPageKindVideo => 'Video';

  @override
  String get vaultPageMkdirHint => 'e.g. Contracts';

  @override
  String get vaultPageMkdirSub => 'Each folder derives its own subkey FSK';

  @override
  String get vaultPageMkdirTitle => 'New encrypted folder';

  @override
  String get vaultPageNameLabel => 'Name';

  @override
  String get vaultPageNewFolder => 'New folder';

  @override
  String get vaultPageNoFiles => 'No files';

  @override
  String get vaultPageNoFilesEmptySub =>
      'This folder is empty — drop files here, or use \"Import files\" below.';

  @override
  String get vaultPageNoFilesSub =>
      'No files at this level — open a subfolder on the left, or use \"Import files\" below.';

  @override
  String get vaultPageNoSubfolders => 'No subfolders';

  @override
  String get vaultPageNoTags => 'No tags';

  @override
  String get vaultPageOpen => 'Open';

  @override
  String get vaultPagePreview => 'Preview';

  @override
  String get vaultPagePreviewNote =>
      'The engine exposes no plaintext preview channel (plaintext never leaves the engine layer); export the file to read it.';

  @override
  String vaultPagePreviewSub(String size) {
    return '$size · decrypted inside the unlocked session';
  }

  @override
  String vaultPagePreviewTitle(String name) {
    return 'Preview · $name';
  }

  @override
  String get vaultPageRename => 'Rename';

  @override
  String get vaultPageRenameFileSub =>
      'Change the file name in the encrypted metadata';

  @override
  String get vaultPageRenameFolderSub =>
      'Change the folder name in the encrypted metadata';

  @override
  String get vaultPageRenamed => 'Renamed · encrypted metadata updated';

  @override
  String get vaultPageRotateFileDone => 'Key rotated · container rewritten';

  @override
  String get vaultPageRotateFileSub =>
      'Rewrite this file\'s container under a fresh random FSKey';

  @override
  String get vaultPageRotateFileTitle => 'Rotate file key';

  @override
  String get vaultPageRotateFolderDone =>
      'Folder key rotated · direct file containers rewritten';

  @override
  String get vaultPageRotateFolderSub =>
      'New FSK override + rewrite of the folder\'s own file containers';

  @override
  String get vaultPageRotateFolderTitle => 'Rotate folder key';

  @override
  String get vaultPageRotateKey => 'Rotate key';

  @override
  String get vaultPageRotateNote =>
      'After rotation the old key path is dead (the new key is random, no longer derived from the MK). This is a local rotation: the vector clock is not advanced, so it does not propagate to paired devices and they keep their own readable copies. Rotating a folder covers its direct files only, not subfolders.';

  @override
  String get vaultPageRotateStart => 'Rotate now';

  @override
  String get vaultPageRotateTargetFile => 'File';

  @override
  String get vaultPageRotateTargetFolder => 'Folder';

  @override
  String get vaultPageSave => 'Save';

  @override
  String vaultPageSearching(String query) {
    return 'Search \"$query\"';
  }

  @override
  String vaultPageSelectedCount(int n) {
    return '$n selected';
  }

  @override
  String get vaultPageShare => 'Burn-after-read share';

  @override
  String get vaultPageShareFailed => 'Could not create the share';

  @override
  String vaultPageShareIdAndToken(String id, String token) {
    return 'ID: $id\nToken: $token';
  }

  @override
  String get vaultPageShareNote =>
      'The share expires as soon as the peer opens it with the token; the token cannot be reused.';

  @override
  String get vaultPageShareSub =>
      'One-time P2P session · wiped shortly after the peer opens it';

  @override
  String get vaultPageShareTokenLabel => 'One-time share token (shown once)';

  @override
  String get vaultPageSizeLabel => 'Size';

  @override
  String get vaultPageStatusLabel => 'Status';

  @override
  String get vaultPageSynced => 'Synced';

  @override
  String get vaultPageTags => 'Tags';

  @override
  String get vaultPageTagsHint => 'Comma-separated, e.g. work,private';

  @override
  String vaultPageTagsSub(String name) {
    return 'Manage the tags of \"$name\"';
  }

  @override
  String get vaultPageTagsTitle => 'Manage tags';

  @override
  String get vaultPageTagsUpdated => 'Tags updated';

  @override
  String get vaultPageTargetFile => 'Target file';

  @override
  String get vaultPageTargetFolder => 'Target folder';

  @override
  String get vaultPageTreeAllFiles => 'All files';

  @override
  String get vaultPageTreeCurrentPath => 'Current path';

  @override
  String get vaultPageTreeQuickAccess => 'Quick access';

  @override
  String get vaultPageTypeLabel => 'Type';

  @override
  String get vaultPageUnnamed => 'Untitled';

  @override
  String get vaultPageUnnamedFile => 'Untitled file';

  @override
  String get vaultPageUnnamedFolder => 'Untitled folder';

  @override
  String vaultPageUnnamedWithId(int id) {
    return 'Untitled-$id';
  }

  @override
  String get vaultPageViewGrid => 'Grid';

  @override
  String get vaultPageViewList => 'List';

  @override
  String get vaultPageViewSplit => 'Split';

  @override
  String get vaultPageWipe => 'Secure erase';

  @override
  String get vaultPageWipeAll => 'Erase all';

  @override
  String get vaultPageWipeConfirm => 'Erase';

  @override
  String vaultPageWipeCount(int n) {
    return 'Securely erase $n files';
  }

  @override
  String get vaultPageWipeFileSub =>
      'Destroy the file key FSKey and unlink its chunk list';

  @override
  String vaultPageWipeFolderFailed(String name, String error, int count) {
    return '\"$name\" $error ($count files destroyed)';
  }

  @override
  String get vaultPageWipeFolderSub =>
      'Recursively destroy the keys and ciphertext of the folder and everything in it';

  @override
  String vaultPageWipeManyDone(int n) {
    return 'Securely erased $n items';
  }

  @override
  String vaultPageWipeManyFailed(int failed, int n) {
    return 'Erase finished: $failed/$n failed';
  }

  @override
  String vaultPageWipeManyFilesSub(int n) {
    return 'Destroy the keys of $n files and delete their ciphertext';
  }

  @override
  String vaultPageWipeManyFoldersSub(int folders) {
    return 'Destroy keys and delete ciphertext ($folders folders, recursive)';
  }

  @override
  String vaultPageWipeManyNote(int n) {
    return 'This erases $n items. It cannot be undone and propagates to every paired device.';
  }

  @override
  String get vaultPageWipeManyTitle => 'Securely erase selected items';

  @override
  String get vaultPageWipeNote =>
      'The erase propagates to every paired device and is written to the audit log.';

  @override
  String get vaultPageWiped => 'Securely erased';

  @override
  String vsStepsDefaultLabel(Object n) {
    return 'Step $n';
  }

  @override
  String get windowChromeClose => 'Close';

  @override
  String get windowChromeMaximize => 'Maximize';

  @override
  String get windowChromeMinimize => 'Minimize';

  @override
  String get windowChromeRestore => 'Restore Down';
}
