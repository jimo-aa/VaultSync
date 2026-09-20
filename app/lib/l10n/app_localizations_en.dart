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
