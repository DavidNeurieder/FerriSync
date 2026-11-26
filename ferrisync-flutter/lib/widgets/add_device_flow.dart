import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_zxing/flutter_zxing.dart';
import '../gen/api.dart' as frb;
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../utils/pairing_qr.dart';
import '../widgets/empty_state.dart';

/// The sequence of stages a pairing attempt moves through. Encapsulates the
/// previously-fragmented scan/pair-by-IP/QR journeys into one state machine
/// shared by the standalone Add-device screen and the onboarding wizard.
enum AddDeviceStage {
  /// Scanning for nearby devices (mDNS) and letting the user pick one.
  scan,
  /// Initial pairing handshake in progress.
  connecting,
  /// Handshake done — waiting for the remote user to approve.
  waitingApproval,
  /// Fully paired and visible in the device list.
  paired,
  /// The remote rejected or timed out.
  failed,
}

/// One "add a device" journey: discover a peer (or scan its QR code), pair,
/// wait for approval, and report the paired device. The caller supplies a
/// `BuildContext` via the provided callbacks so this stays presentation-only
/// and still ties into navigation/snackbars from whichever screen embeds it.
class AddDeviceFlow extends StatefulWidget {
  const AddDeviceFlow({
    super.key,
    required this.service,
    required this.onPaired,
    this.onCancelled,
    this.compact = false,
  });

  final SyncService service;
  final ValueChanged<frb.DiscoveredDevice?> onPaired;
  final VoidCallback? onCancelled;

  /// In onboarding there is no app bar; the widget renders its own heading.
  final bool compact;

  @override
  State<AddDeviceFlow> createState() => _AddDeviceFlowState();
}

class _AddDeviceFlowState extends State<AddDeviceFlow> {
  AddDeviceStage _stage = AddDeviceStage.scan;
  List<frb.DiscoveredDevice> _devices = [];
  bool _scanning = true;
  int? _pairingIndex;
  String? _error;
  String? _pairedMessage;
  bool _pairedHandled = false;

  @override
  void initState() {
    super.initState();
    _startScan();
  }

  Future<void> _startScan() async {
    setState(() {
      _scanning = true;
      _error = null;
    });
    try {
      final devices = await widget.service.discoverDevices(timeoutSecs: 4);
      if (!mounted) return;
      setState(() {
        _devices = devices;
        _scanning = false;
      });
    } catch (_) {
      if (!mounted) return;
      setState(() {
        _devices = [];
        _scanning = false;
      });
    }
  }

  Future<void> _pairWith(frb.DiscoveredDevice? d, {String? ip, int? port}) async {
    setState(() {
      _stage = AddDeviceStage.connecting;
      _error = null;
    });
    String message;
    try {
      message = d != null
          ? await widget.service.pairWithDeviceAndFolders(
              d.ip,
              d.controlPort ?? d.port,
              chooseFolders: _chooseFolders,
            )
          : await widget.service.pairWithDeviceAndFolders(
              ip!,
              port ?? 9847,
              chooseFolders: _chooseFolders,
            );
    } catch (e) {
      if (mounted) {
        setState(() {
          _stage = AddDeviceStage.failed;
          _error = 'Pairing failed: $e';
        });
      }
      return;
    }

    if (!mounted) return;
    await widget.service.refresh();

    // The pair call resolves either as "paired" or "waiting for approval".
    // Re-check against the device list to know which.
    final isPaired = _deviceListContains(widget.service.devices, d);
    if (isPaired) {
      _finishPaired(d, message: message);
      return;
    }

    // Still waiting for the remote to approve — poll until it disappears into
    // the paired list or times out.
    setState(() => _stage = AddDeviceStage.waitingApproval);
    final deadline = DateTime.now().add(const Duration(seconds: 45));
    while (DateTime.now().isBefore(deadline)) {
      await widget.service.pollPendingPairings();
      await widget.service.refresh();
      if (!mounted) return;
      if (_deviceListContains(widget.service.devices, d)) {
        _finishPaired(d, message: message);
        return;
      }
      await Future.delayed(const Duration(seconds: 2));
    }
    if (mounted) {
      setState(() {
        _stage = AddDeviceStage.failed;
        _error = 'No approval received — check the other device and try again.';
      });
    }
  }

  /// Resolve an ambiguous folder pairing. Called only when one side publishes
  /// more than one share; a sole share on each side is paired without a prompt.
  Future<FolderPairChoice?> _chooseFolders(
    List<frb.SharedFolder> localShares,
    List<frb.RemoteSharedFolder> remoteShares,
  ) async {
    frb.SharedFolder? selectedLocal =
        localShares.length == 1 ? localShares.first : null;
    frb.RemoteSharedFolder? selectedRemote =
        remoteShares.isEmpty ? null : remoteShares.first;

    return showDialog<FolderPairChoice>(
      context: context,
      builder: (ctx) {
        return StatefulBuilder(
          builder: (ctx, setState) {
            return AlertDialog(
              title: const Text('Choose folders to pair'),
              content: SingleChildScrollView(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      remoteShares.isEmpty
                          ? 'The other device has no shared folders. Pick '
                              'which of yours to offer.'
                          : localShares.isEmpty
                              ? 'This device has no shared folders. Pick '
                                  'which of the other device\'s folders to adopt.'
                              : 'At least one device publishes several '
                                  'folders. Pick which pair should start syncing.',
                    ),
                    const SizedBox(height: FerriTokens.spaceL),
                    DropdownButtonFormField<frb.SharedFolder?>(
                      initialValue: selectedLocal,
                      decoration: const InputDecoration(
                        labelText: 'Folder on this device',
                        border: OutlineInputBorder(),
                      ),
                      items: [
                        const DropdownMenuItem<frb.SharedFolder?>(
                          value: null,
                          child: Text('Create a new folder'),
                        ),
                        ...localShares.map(
                          (f) => DropdownMenuItem<frb.SharedFolder?>(
                            value: f,
                            child: Text(f.name),
                          ),
                        ),
                      ],
                      onChanged: (value) => setState(() => selectedLocal = value),
                    ),
                    const SizedBox(height: FerriTokens.spaceS),
                    DropdownButtonFormField<frb.RemoteSharedFolder?>(
                      initialValue: selectedRemote,
                      decoration: const InputDecoration(
                        labelText: 'Folder on the other device',
                        border: OutlineInputBorder(),
                      ),
                      items: [
                        if (remoteShares.isEmpty)
                          const DropdownMenuItem<frb.RemoteSharedFolder?>(
                            value: null,
                            child: Text('None — offer this device\'s folder'),
                          ),
                        for (final f in remoteShares)
                          DropdownMenuItem<frb.RemoteSharedFolder?>(
                            value: f,
                            child: Text(f.name),
                          ),
                      ],
                      onChanged: (value) {
                        setState(() => selectedRemote = value);
                      },
                    ),
                  ],
                ),
              ),
              actions: [
                TextButton(
                  onPressed: () => Navigator.of(ctx).pop(),
                  child: const Text('Cancel'),
                ),
                FilledButton(
                  onPressed: () => Navigator.of(ctx).pop<FolderPairChoice>(
                        (local: selectedLocal, remote: selectedRemote),
                      ),
                  child: const Text('Pair folders'),
                ),
              ],
            );
          },
        );
      },
    );
  }

  bool _deviceListContains(List<Device>? devices, frb.DiscoveredDevice? d) {
    if (d == null || devices == null) return false;
    for (final device in devices) {
      if (device.id == d.id) return true;
    }
    return false;
  }

  void _finishPaired(frb.DiscoveredDevice? d, {String? message}) {
    if (_pairedHandled) return;
    _pairedHandled = true;
    if (!mounted) return;
    setState(() {
      _stage = AddDeviceStage.paired;
      _pairedMessage = message;
    });
    // Defer the callback so the "paired" screen renders first.
    Future.microtask(() => widget.onPaired(d));
  }

  void _showScanner() {
    Navigator.push(
      context,
      MaterialPageRoute(
        builder: (_) => _QrScanPage(
          onScanned: (value) {
            final parts = value.split(':');
            final ip = parts.isNotEmpty ? parts[0].trim() : '';
            final port =
                parts.length > 1 ? int.tryParse(parts[1]) ?? 9847 : 9847;
            if (ip.isNotEmpty) _pairWith(null, ip: ip, port: port);
          },
        ),
      ),
    );
  }

  Future<void> _showQr() async {
    final addresses = await widget.service.localPairingAddresses();
    if (!mounted) return;
    if (addresses.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('Share a folder first — there is no address to show yet.'),
        ),
      );
      return;
    }
    await Navigator.push(
      context,
      MaterialPageRoute(
        builder: (_) => _QrDisplayPage(address: addresses.first),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return switch (_stage) {
      AddDeviceStage.scan => _buildScan(),
      AddDeviceStage.connecting =>
        _buildStage(Icons.sync, 'Connecting…', 'Starting the pairing handshake…'),
      AddDeviceStage.waitingApproval => _buildStage(
          Icons.hourglass_top,
          'Waiting for approval',
          'Open FerriSync on the other device and allow this connection.',
          progress: true,
        ),
      AddDeviceStage.paired => _buildStage(
          Icons.check_circle_outline,
          'Paired',
          _pairedMessage ?? 'The other device accepted this connection.'),
      AddDeviceStage.failed => _buildFailed(),
    };
  }

  Widget _buildScan() {
    final palette = context.ferri;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (widget.compact) ...[
          Text(
            'Find your devices',
            style: Theme.of(context)
                .textTheme
                .headlineSmall
                ?.copyWith(fontWeight: FontWeight.w700),
          ),
          const SizedBox(height: 4),
          Text(
            'Searching your local network…',
            style: Theme.of(context)
                .textTheme
                .bodyMedium
                ?.copyWith(color: palette.muted),
          ),
          const SizedBox(height: FerriTokens.spaceL),
        ],
        if (_scanning)
          const Padding(
            padding: EdgeInsets.symmetric(vertical: 48),
            child: Center(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  CircularProgressIndicator(),
                  SizedBox(height: 16),
                  Text('Scanning for FerriSync devices…'),
                ],
              ),
            ),
          )
        else if (_devices.isEmpty)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 24),
            child: EmptyState(
              icon: Icons.wifi_find,
              title: 'No devices found',
              subtitle: 'Make sure the other device is on the same network '
                  'and running FerriSync.',
              action: FilledButton.icon(
                onPressed: _startScan,
                icon: const Icon(Icons.refresh),
                label: const Text('Scan again'),
              ),
            ),
          )
        else
          Column(
            children: [
              for (var i = 0; i < _devices.length; i++) ...[
                _deviceTile(i),
                const SizedBox(height: FerriTokens.spaceS),
              ],
            ],
          ),
        const SizedBox(height: FerriTokens.spaceL),
        OutlinedButton.icon(
          onPressed: _showScanner,
          icon: const Icon(Icons.qr_code_scanner, size: 18),
          label: const Text('Pair with a QR code'),
        ),
        const SizedBox(height: 8),
        OutlinedButton.icon(
          onPressed: _showQr,
          icon: const Icon(Icons.qr_code_2, size: 18),
          label: const Text('Show QR code'),
        ),
      ],
    );
  }

  Widget _deviceTile(int index) {
    final palette = context.ferri;
    final d = _devices[index];
    final pairing = _pairingIndex == index;
    return Card(
      color: palette.surfaceHigh,
      child: ListTile(
        onTap: pairing ? null : () => _confirmPair(index),
        leading: Icon(_deviceTypeIcon(d.name), color: palette.primary),
        title: Text(d.name),
        subtitle: const Text('Available · tap to pair'),
        trailing: pairing
            ? const SizedBox(
                width: 20,
                height: 20,
                child: CircularProgressIndicator(strokeWidth: 2),
              )
            : const Icon(Icons.chevron_right, size: 20),
      ),
    );
  }

  IconData _deviceTypeIcon(String name) {
    final n = name.toLowerCase();
    if (n.contains('laptop') || n.contains('desktop') || n.contains('pc')) {
      return Icons.laptop_mac;
    }
    if (n.contains('phone') ||
        n.contains('pixel') ||
        n.contains('sm-g') ||
        n.contains('motorola')) {
      return Icons.smartphone;
    }
    if (n.contains('server') || n.contains('nas') || n.contains('cloud')) {
      return Icons.dns;
    }
    return Icons.devices_other;
  }

  /// Ask for confirmation, then start pairing. Explains what pairing allows so
  /// the user never has to think about addresses or certificates.
  Future<void> _confirmPair(int index) async {
    final d = _devices[index];
    final agreed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Pair with ${d.name}?'),
        content: const Text(
          'This will allow your devices to synchronize files directly '
          'over your local network.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Pair'),
          ),
        ],
      ),
    );
    if (agreed != true || !mounted) return;
    setState(() => _pairingIndex = index);
    _pairWith(d);
  }

  Widget _buildStage(IconData icon, String title, String subtitle,
      {bool progress = false}) {
    final palette = context.ferri;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 40),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          progress
              ? const CircularProgressIndicator()
              : Icon(icon, size: 48, color: palette.success),
          const SizedBox(height: FerriTokens.spaceL),
          Text(
            title,
            style: Theme.of(context)
                .textTheme
                .titleLarge
                ?.copyWith(fontWeight: FontWeight.w700),
          ),
          const SizedBox(height: FerriTokens.spaceS),
          Text(
            subtitle,
            textAlign: TextAlign.center,
            style: Theme.of(context)
                .textTheme
                .bodyMedium
                ?.copyWith(color: palette.muted),
          ),
          const SizedBox(height: FerriTokens.spaceL),
          TextButton(
            onPressed: widget.onCancelled,
            child: const Text('Cancel'),
          ),
        ],
      ),
    );
  }

  Widget _buildFailed() {
    final palette = context.ferri;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 32),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.error_outline, size: 48, color: palette.danger),
          const SizedBox(height: FerriTokens.spaceL),
          Text(
            'Couldn\'t pair',
            style: Theme.of(context)
                .textTheme
                .titleLarge
                ?.copyWith(fontWeight: FontWeight.w700),
          ),
          const SizedBox(height: FerriTokens.spaceS),
          Text(
            _error ?? 'Something went wrong.',
            textAlign: TextAlign.center,
            style: Theme.of(context)
                .textTheme
                .bodyMedium
                ?.copyWith(color: palette.muted),
          ),
          const SizedBox(height: FerriTokens.spaceL),
          Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              TextButton(
                onPressed: () {
                  setState(() {
                    _stage = AddDeviceStage.scan;
                    _pairedHandled = false;
                    _startScan();
                  });
                },
                child: const Text('Try again'),
              ),
              const SizedBox(width: 8),
              TextButton(onPressed: widget.onCancelled, child: const Text('Cancel')),
            ],
          ),
        ],
      ),
    );
  }
}

class _QrScanPage extends StatefulWidget {
  const _QrScanPage({required this.onScanned});

  final ValueChanged<String> onScanned;

  @override
  State<_QrScanPage> createState() => _QrScanPageState();
}

class _QrScanPageState extends State<_QrScanPage> {
  bool _handled = false;

  void _handle(Code result) {
    if (_handled || !result.isValid) return;
    final value = result.text;
    if (value == null || value.isEmpty) return;
    _handled = true;
    Navigator.pop(context);
    widget.onScanned(value);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Scan QR Code')),
      body: ReaderWidget(
        onScan: _handle,
        showGallery: false,
        showToggleCamera: false,
      ),
    );
  }
}

class _QrDisplayPage extends StatefulWidget {
  const _QrDisplayPage({required this.address});

  final String address;

  @override
  State<_QrDisplayPage> createState() => _QrDisplayPageState();
}

class _QrDisplayPageState extends State<_QrDisplayPage> {
  Uint8List? _png;
  String? _error;

  @override
  void initState() {
    super.initState();
    _buildQr();
  }

  Future<void> _buildQr() async {
    try {
      final png = await encodePairingQr(widget.address);
      if (!mounted) return;
      setState(() {
        if (png == null) {
          _error = 'Could not generate a QR code for this address.';
        } else {
          _png = png;
        }
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = 'Could not generate a QR code: $e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final palette = context.ferri;
    return Scaffold(
      appBar: AppBar(title: const Text('My pairing QR')),
      body: Center(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(FerriTokens.spaceL),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (_png != null)
                Image.memory(_png!, width: 260, height: 260, fit: BoxFit.contain)
              else if (_error != null)
                Icon(Icons.error_outline, size: 48, color: palette.danger)
              else
                const CircularProgressIndicator(),
              const SizedBox(height: FerriTokens.spaceL),
              SelectableText(
                widget.address,
                style: Theme.of(context)
                    .textTheme
                    .titleMedium
                    ?.copyWith(fontWeight: FontWeight.w700),
              ),
              const SizedBox(height: FerriTokens.spaceS),
              TextButton.icon(
                onPressed: () =>
                    Clipboard.setData(ClipboardData(text: widget.address)),
                icon: const Icon(Icons.copy, size: 18),
                label: const Text('Copy address'),
              ),
              const SizedBox(height: FerriTokens.spaceL),
              Text(
                'Have the other device scan this code and start pairing. '
                'You will approve the request here.',
                textAlign: TextAlign.center,
                style: Theme.of(context)
                    .textTheme
                    .bodyMedium
                    ?.copyWith(color: palette.muted),
              ),
              if (_error != null) ...[
                const SizedBox(height: FerriTokens.spaceS),
                Text(
                  _error!,
                  textAlign: TextAlign.center,
                  style: Theme.of(context)
                      .textTheme
                      .bodySmall
                      ?.copyWith(color: palette.danger),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
