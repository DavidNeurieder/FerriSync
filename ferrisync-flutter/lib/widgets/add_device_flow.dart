import 'dart:async';

import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';
import '../gen/api.dart' as frb;
import '../models/sync_models.dart';
import '../providers/sync_provider.dart';
import '../theme/ferri_theme.dart';
import '../widgets/empty_state.dart';

/// The sequence of stages a pairing attempt moves through. Encapsulates the
/// previously-fragmented scan/pair-by-IP/QR journeys into one state machine
/// shared by the standalone Add-device screen and the onboarding wizard.
enum AddDeviceStage {
  /// Scanning for nearby devices (mDNS) and letting the user pick one.
  scan,
  /// Manually entering an IP/port (advanced path).
  manual,
  /// Initial pairing handshake in progress.
  connecting,
  /// Handshake done — waiting for the remote user to approve.
  waitingApproval,
  /// Fully paired and visible in the device list.
  paired,
  /// The remote rejected or timed out.
  failed,
}

/// One "add a device" journey: pick a peer (or enter one manually), pair,
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
    try {
      d != null
          ? await widget.service.pairWithDevice(d.ip, d.port)
          : await widget.service.pairWithDevice(ip!, port ?? 9847);
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
      _finishPaired(d);
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
        _finishPaired(d);
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

  bool _deviceListContains(List<Device>? devices, frb.DiscoveredDevice? d) {
    if (d == null || devices == null) return false;
    for (final device in devices) {
      if (device.id == d.id) return true;
    }
    return false;
  }

  void _finishPaired(frb.DiscoveredDevice? d) {
    if (_pairedHandled) return;
    _pairedHandled = true;
    if (!mounted) return;
    setState(() => _stage = AddDeviceStage.paired);
    // Defer the callback so the "paired" screen renders first.
    Future.microtask(() => widget.onPaired(d));
  }

  void _goManual() {
    setState(() {
      _stage = AddDeviceStage.manual;
      _error = null;
    });
  }

  void _showScanner() {
    Navigator.push(
      context,
      MaterialPageRoute(
        builder: (_) => Scaffold(
          appBar: AppBar(title: const Text('Scan QR Code')),
          body: MobileScanner(
            onDetect: (capture) async {
              final barcode = capture.barcodes.firstOrNull;
              if (barcode?.rawValue case final value?) {
                Navigator.pop(context);
                final parts = value.split(':');
                final ip = parts.isNotEmpty ? parts[0] : '';
                final port = parts.length > 1
                    ? int.tryParse(parts[1]) ?? 9847
                    : 9847;
                if (ip.isNotEmpty) _pairWith(null, ip: ip, port: port);
              }
            },
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return switch (_stage) {
      AddDeviceStage.scan => _buildScan(),
      AddDeviceStage.manual => _buildManual(),
      AddDeviceStage.connecting =>
        _buildStage(Icons.sync, 'Connecting…', 'Starting the pairing handshake…'),
      AddDeviceStage.waitingApproval => _buildStage(
          Icons.hourglass_top,
          'Waiting for approval',
          'Open FerriSync on the other device and allow this connection.',
          progress: true,
        ),
      AddDeviceStage.paired =>
        _buildStage(Icons.check_circle_outline, 'Paired', 'The other device accepted this connection.'),
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
        TextButton.icon(
          onPressed: _goManual,
          icon: const Icon(Icons.keyboard_alt_outlined, size: 18),
          label: const Text('Enter address manually'),
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
        leading: Icon(Icons.dns, color: palette.primary),
        title: Text(d.name),
        subtitle: Text('${d.ip}:${d.port}'),
        trailing: pairing
            ? const SizedBox(
                width: 20,
                height: 20,
                child: CircularProgressIndicator(strokeWidth: 2),
              )
            : FilledButton(
                onPressed: () {
                  setState(() => _pairingIndex = index);
                  _pairWith(d);
                },
                child: const Text('Connect'),
              ),
      ),
    );
  }

  Widget _buildManual() {
    final ipCtrl = TextEditingController();
    final portCtrl = TextEditingController(text: '9847');
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (widget.compact) ...[
          Text(
            'Add a device',
            style: Theme.of(context)
                .textTheme
                .headlineSmall
                ?.copyWith(fontWeight: FontWeight.w700),
          ),
          const SizedBox(height: FerriTokens.spaceL),
        ],
        TextField(
          controller: ipCtrl,
          decoration: const InputDecoration(
            labelText: 'IP address',
            hintText: '192.168.1.x',
          ),
        ),
        const SizedBox(height: FerriTokens.spaceM),
        TextField(
          controller: portCtrl,
          decoration: const InputDecoration(labelText: 'Port'),
          keyboardType: TextInputType.number,
        ),
        const SizedBox(height: FerriTokens.spaceM),
        OutlinedButton.icon(
          onPressed: _showScanner,
          icon: const Icon(Icons.qr_code_scanner),
          label: const Text('Scan QR Code'),
        ),
        const SizedBox(height: FerriTokens.spaceL),
        Row(
          children: [
            TextButton(
              onPressed: () {
                setState(() {
                  _stage = AddDeviceStage.scan;
                  _startScan();
                });
              },
              child: const Text('Back'),
            ),
            const Spacer(),
            FilledButton.icon(
              onPressed: () {
                final ip = ipCtrl.text.trim();
                if (ip.isEmpty) return;
                _pairWith(null, ip: ip, port: int.tryParse(portCtrl.text) ?? 9847);
              },
              icon: const Icon(Icons.link, size: 18),
              label: const Text('Connect'),
            ),
          ],
        ),
      ],
    );
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
