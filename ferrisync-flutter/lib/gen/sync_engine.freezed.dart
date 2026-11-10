// coverage:ignore-file
// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'sync_engine.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

T _$identity<T>(T value) => value;

final _privateConstructorUsedError = UnsupportedError(
    'It seems like you constructed your class using `MyClass._()`. This constructor is only meant to be used by freezed and you are not supposed to need it nor use it.\nPlease check the documentation here for more information: https://github.com/rrousselGit/freezed#adding-getters-and-methods-to-our-models');

/// @nodoc
mixin _$SyncEvent {
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String folderId) syncing,
    required TResult Function() idle,
    required TResult Function(String message) error,
    required TResult Function(String name, String id) pairRequested,
    required TResult Function(String name, String id, String folder)
        folderPairRequested,
    required TResult Function(String name, String id) devicePaired,
    required TResult Function(String path, String device) filePulled,
    required TResult Function(String path, String device) filePushed,
    required TResult Function(String path, String winner, String loser)
        conflict,
    required TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)
        progress,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String folderId)? syncing,
    TResult? Function()? idle,
    TResult? Function(String message)? error,
    TResult? Function(String name, String id)? pairRequested,
    TResult? Function(String name, String id, String folder)?
        folderPairRequested,
    TResult? Function(String name, String id)? devicePaired,
    TResult? Function(String path, String device)? filePulled,
    TResult? Function(String path, String device)? filePushed,
    TResult? Function(String path, String winner, String loser)? conflict,
    TResult? Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String folderId)? syncing,
    TResult Function()? idle,
    TResult Function(String message)? error,
    TResult Function(String name, String id)? pairRequested,
    TResult Function(String name, String id, String folder)?
        folderPairRequested,
    TResult Function(String name, String id)? devicePaired,
    TResult Function(String path, String device)? filePulled,
    TResult Function(String path, String device)? filePushed,
    TResult Function(String path, String winner, String loser)? conflict,
    TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(SyncEvent_Syncing value) syncing,
    required TResult Function(SyncEvent_Idle value) idle,
    required TResult Function(SyncEvent_Error value) error,
    required TResult Function(SyncEvent_PairRequested value) pairRequested,
    required TResult Function(SyncEvent_FolderPairRequested value)
        folderPairRequested,
    required TResult Function(SyncEvent_DevicePaired value) devicePaired,
    required TResult Function(SyncEvent_FilePulled value) filePulled,
    required TResult Function(SyncEvent_FilePushed value) filePushed,
    required TResult Function(SyncEvent_Conflict value) conflict,
    required TResult Function(SyncEvent_Progress value) progress,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SyncEvent_Syncing value)? syncing,
    TResult? Function(SyncEvent_Idle value)? idle,
    TResult? Function(SyncEvent_Error value)? error,
    TResult? Function(SyncEvent_PairRequested value)? pairRequested,
    TResult? Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult? Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult? Function(SyncEvent_FilePulled value)? filePulled,
    TResult? Function(SyncEvent_FilePushed value)? filePushed,
    TResult? Function(SyncEvent_Conflict value)? conflict,
    TResult? Function(SyncEvent_Progress value)? progress,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SyncEvent_Syncing value)? syncing,
    TResult Function(SyncEvent_Idle value)? idle,
    TResult Function(SyncEvent_Error value)? error,
    TResult Function(SyncEvent_PairRequested value)? pairRequested,
    TResult Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult Function(SyncEvent_FilePulled value)? filePulled,
    TResult Function(SyncEvent_FilePushed value)? filePushed,
    TResult Function(SyncEvent_Conflict value)? conflict,
    TResult Function(SyncEvent_Progress value)? progress,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class $SyncEventCopyWith<$Res> {
  factory $SyncEventCopyWith(SyncEvent value, $Res Function(SyncEvent) then) =
      _$SyncEventCopyWithImpl<$Res, SyncEvent>;
}

/// @nodoc
class _$SyncEventCopyWithImpl<$Res, $Val extends SyncEvent>
    implements $SyncEventCopyWith<$Res> {
  _$SyncEventCopyWithImpl(this._value, this._then);

  // ignore: unused_field
  final $Val _value;
  // ignore: unused_field
  final $Res Function($Val) _then;

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
}

/// @nodoc
abstract class _$$SyncEvent_SyncingImplCopyWith<$Res> {
  factory _$$SyncEvent_SyncingImplCopyWith(_$SyncEvent_SyncingImpl value,
          $Res Function(_$SyncEvent_SyncingImpl) then) =
      __$$SyncEvent_SyncingImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String folderId});
}

/// @nodoc
class __$$SyncEvent_SyncingImplCopyWithImpl<$Res>
    extends _$SyncEventCopyWithImpl<$Res, _$SyncEvent_SyncingImpl>
    implements _$$SyncEvent_SyncingImplCopyWith<$Res> {
  __$$SyncEvent_SyncingImplCopyWithImpl(_$SyncEvent_SyncingImpl _value,
      $Res Function(_$SyncEvent_SyncingImpl) _then)
      : super(_value, _then);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? folderId = null,
  }) {
    return _then(_$SyncEvent_SyncingImpl(
      folderId: null == folderId
          ? _value.folderId
          : folderId // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$SyncEvent_SyncingImpl extends SyncEvent_Syncing {
  const _$SyncEvent_SyncingImpl({required this.folderId}) : super._();

  @override
  final String folderId;

  @override
  String toString() {
    return 'SyncEvent.syncing(folderId: $folderId)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$SyncEvent_SyncingImpl &&
            (identical(other.folderId, folderId) ||
                other.folderId == folderId));
  }

  @override
  int get hashCode => Object.hash(runtimeType, folderId);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$SyncEvent_SyncingImplCopyWith<_$SyncEvent_SyncingImpl> get copyWith =>
      __$$SyncEvent_SyncingImplCopyWithImpl<_$SyncEvent_SyncingImpl>(
          this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String folderId) syncing,
    required TResult Function() idle,
    required TResult Function(String message) error,
    required TResult Function(String name, String id) pairRequested,
    required TResult Function(String name, String id, String folder)
        folderPairRequested,
    required TResult Function(String name, String id) devicePaired,
    required TResult Function(String path, String device) filePulled,
    required TResult Function(String path, String device) filePushed,
    required TResult Function(String path, String winner, String loser)
        conflict,
    required TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)
        progress,
  }) {
    return syncing(folderId);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String folderId)? syncing,
    TResult? Function()? idle,
    TResult? Function(String message)? error,
    TResult? Function(String name, String id)? pairRequested,
    TResult? Function(String name, String id, String folder)?
        folderPairRequested,
    TResult? Function(String name, String id)? devicePaired,
    TResult? Function(String path, String device)? filePulled,
    TResult? Function(String path, String device)? filePushed,
    TResult? Function(String path, String winner, String loser)? conflict,
    TResult? Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
  }) {
    return syncing?.call(folderId);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String folderId)? syncing,
    TResult Function()? idle,
    TResult Function(String message)? error,
    TResult Function(String name, String id)? pairRequested,
    TResult Function(String name, String id, String folder)?
        folderPairRequested,
    TResult Function(String name, String id)? devicePaired,
    TResult Function(String path, String device)? filePulled,
    TResult Function(String path, String device)? filePushed,
    TResult Function(String path, String winner, String loser)? conflict,
    TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
    required TResult orElse(),
  }) {
    if (syncing != null) {
      return syncing(folderId);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(SyncEvent_Syncing value) syncing,
    required TResult Function(SyncEvent_Idle value) idle,
    required TResult Function(SyncEvent_Error value) error,
    required TResult Function(SyncEvent_PairRequested value) pairRequested,
    required TResult Function(SyncEvent_FolderPairRequested value)
        folderPairRequested,
    required TResult Function(SyncEvent_DevicePaired value) devicePaired,
    required TResult Function(SyncEvent_FilePulled value) filePulled,
    required TResult Function(SyncEvent_FilePushed value) filePushed,
    required TResult Function(SyncEvent_Conflict value) conflict,
    required TResult Function(SyncEvent_Progress value) progress,
  }) {
    return syncing(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SyncEvent_Syncing value)? syncing,
    TResult? Function(SyncEvent_Idle value)? idle,
    TResult? Function(SyncEvent_Error value)? error,
    TResult? Function(SyncEvent_PairRequested value)? pairRequested,
    TResult? Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult? Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult? Function(SyncEvent_FilePulled value)? filePulled,
    TResult? Function(SyncEvent_FilePushed value)? filePushed,
    TResult? Function(SyncEvent_Conflict value)? conflict,
    TResult? Function(SyncEvent_Progress value)? progress,
  }) {
    return syncing?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SyncEvent_Syncing value)? syncing,
    TResult Function(SyncEvent_Idle value)? idle,
    TResult Function(SyncEvent_Error value)? error,
    TResult Function(SyncEvent_PairRequested value)? pairRequested,
    TResult Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult Function(SyncEvent_FilePulled value)? filePulled,
    TResult Function(SyncEvent_FilePushed value)? filePushed,
    TResult Function(SyncEvent_Conflict value)? conflict,
    TResult Function(SyncEvent_Progress value)? progress,
    required TResult orElse(),
  }) {
    if (syncing != null) {
      return syncing(this);
    }
    return orElse();
  }
}

abstract class SyncEvent_Syncing extends SyncEvent {
  const factory SyncEvent_Syncing({required final String folderId}) =
      _$SyncEvent_SyncingImpl;
  const SyncEvent_Syncing._() : super._();

  String get folderId;

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$SyncEvent_SyncingImplCopyWith<_$SyncEvent_SyncingImpl> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$SyncEvent_IdleImplCopyWith<$Res> {
  factory _$$SyncEvent_IdleImplCopyWith(_$SyncEvent_IdleImpl value,
          $Res Function(_$SyncEvent_IdleImpl) then) =
      __$$SyncEvent_IdleImplCopyWithImpl<$Res>;
}

/// @nodoc
class __$$SyncEvent_IdleImplCopyWithImpl<$Res>
    extends _$SyncEventCopyWithImpl<$Res, _$SyncEvent_IdleImpl>
    implements _$$SyncEvent_IdleImplCopyWith<$Res> {
  __$$SyncEvent_IdleImplCopyWithImpl(
      _$SyncEvent_IdleImpl _value, $Res Function(_$SyncEvent_IdleImpl) _then)
      : super(_value, _then);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
}

/// @nodoc

class _$SyncEvent_IdleImpl extends SyncEvent_Idle {
  const _$SyncEvent_IdleImpl() : super._();

  @override
  String toString() {
    return 'SyncEvent.idle()';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType && other is _$SyncEvent_IdleImpl);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String folderId) syncing,
    required TResult Function() idle,
    required TResult Function(String message) error,
    required TResult Function(String name, String id) pairRequested,
    required TResult Function(String name, String id, String folder)
        folderPairRequested,
    required TResult Function(String name, String id) devicePaired,
    required TResult Function(String path, String device) filePulled,
    required TResult Function(String path, String device) filePushed,
    required TResult Function(String path, String winner, String loser)
        conflict,
    required TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)
        progress,
  }) {
    return idle();
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String folderId)? syncing,
    TResult? Function()? idle,
    TResult? Function(String message)? error,
    TResult? Function(String name, String id)? pairRequested,
    TResult? Function(String name, String id, String folder)?
        folderPairRequested,
    TResult? Function(String name, String id)? devicePaired,
    TResult? Function(String path, String device)? filePulled,
    TResult? Function(String path, String device)? filePushed,
    TResult? Function(String path, String winner, String loser)? conflict,
    TResult? Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
  }) {
    return idle?.call();
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String folderId)? syncing,
    TResult Function()? idle,
    TResult Function(String message)? error,
    TResult Function(String name, String id)? pairRequested,
    TResult Function(String name, String id, String folder)?
        folderPairRequested,
    TResult Function(String name, String id)? devicePaired,
    TResult Function(String path, String device)? filePulled,
    TResult Function(String path, String device)? filePushed,
    TResult Function(String path, String winner, String loser)? conflict,
    TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
    required TResult orElse(),
  }) {
    if (idle != null) {
      return idle();
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(SyncEvent_Syncing value) syncing,
    required TResult Function(SyncEvent_Idle value) idle,
    required TResult Function(SyncEvent_Error value) error,
    required TResult Function(SyncEvent_PairRequested value) pairRequested,
    required TResult Function(SyncEvent_FolderPairRequested value)
        folderPairRequested,
    required TResult Function(SyncEvent_DevicePaired value) devicePaired,
    required TResult Function(SyncEvent_FilePulled value) filePulled,
    required TResult Function(SyncEvent_FilePushed value) filePushed,
    required TResult Function(SyncEvent_Conflict value) conflict,
    required TResult Function(SyncEvent_Progress value) progress,
  }) {
    return idle(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SyncEvent_Syncing value)? syncing,
    TResult? Function(SyncEvent_Idle value)? idle,
    TResult? Function(SyncEvent_Error value)? error,
    TResult? Function(SyncEvent_PairRequested value)? pairRequested,
    TResult? Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult? Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult? Function(SyncEvent_FilePulled value)? filePulled,
    TResult? Function(SyncEvent_FilePushed value)? filePushed,
    TResult? Function(SyncEvent_Conflict value)? conflict,
    TResult? Function(SyncEvent_Progress value)? progress,
  }) {
    return idle?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SyncEvent_Syncing value)? syncing,
    TResult Function(SyncEvent_Idle value)? idle,
    TResult Function(SyncEvent_Error value)? error,
    TResult Function(SyncEvent_PairRequested value)? pairRequested,
    TResult Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult Function(SyncEvent_FilePulled value)? filePulled,
    TResult Function(SyncEvent_FilePushed value)? filePushed,
    TResult Function(SyncEvent_Conflict value)? conflict,
    TResult Function(SyncEvent_Progress value)? progress,
    required TResult orElse(),
  }) {
    if (idle != null) {
      return idle(this);
    }
    return orElse();
  }
}

abstract class SyncEvent_Idle extends SyncEvent {
  const factory SyncEvent_Idle() = _$SyncEvent_IdleImpl;
  const SyncEvent_Idle._() : super._();
}

/// @nodoc
abstract class _$$SyncEvent_ErrorImplCopyWith<$Res> {
  factory _$$SyncEvent_ErrorImplCopyWith(_$SyncEvent_ErrorImpl value,
          $Res Function(_$SyncEvent_ErrorImpl) then) =
      __$$SyncEvent_ErrorImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String message});
}

/// @nodoc
class __$$SyncEvent_ErrorImplCopyWithImpl<$Res>
    extends _$SyncEventCopyWithImpl<$Res, _$SyncEvent_ErrorImpl>
    implements _$$SyncEvent_ErrorImplCopyWith<$Res> {
  __$$SyncEvent_ErrorImplCopyWithImpl(
      _$SyncEvent_ErrorImpl _value, $Res Function(_$SyncEvent_ErrorImpl) _then)
      : super(_value, _then);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? message = null,
  }) {
    return _then(_$SyncEvent_ErrorImpl(
      message: null == message
          ? _value.message
          : message // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$SyncEvent_ErrorImpl extends SyncEvent_Error {
  const _$SyncEvent_ErrorImpl({required this.message}) : super._();

  @override
  final String message;

  @override
  String toString() {
    return 'SyncEvent.error(message: $message)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$SyncEvent_ErrorImpl &&
            (identical(other.message, message) || other.message == message));
  }

  @override
  int get hashCode => Object.hash(runtimeType, message);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$SyncEvent_ErrorImplCopyWith<_$SyncEvent_ErrorImpl> get copyWith =>
      __$$SyncEvent_ErrorImplCopyWithImpl<_$SyncEvent_ErrorImpl>(
          this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String folderId) syncing,
    required TResult Function() idle,
    required TResult Function(String message) error,
    required TResult Function(String name, String id) pairRequested,
    required TResult Function(String name, String id, String folder)
        folderPairRequested,
    required TResult Function(String name, String id) devicePaired,
    required TResult Function(String path, String device) filePulled,
    required TResult Function(String path, String device) filePushed,
    required TResult Function(String path, String winner, String loser)
        conflict,
    required TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)
        progress,
  }) {
    return error(message);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String folderId)? syncing,
    TResult? Function()? idle,
    TResult? Function(String message)? error,
    TResult? Function(String name, String id)? pairRequested,
    TResult? Function(String name, String id, String folder)?
        folderPairRequested,
    TResult? Function(String name, String id)? devicePaired,
    TResult? Function(String path, String device)? filePulled,
    TResult? Function(String path, String device)? filePushed,
    TResult? Function(String path, String winner, String loser)? conflict,
    TResult? Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
  }) {
    return error?.call(message);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String folderId)? syncing,
    TResult Function()? idle,
    TResult Function(String message)? error,
    TResult Function(String name, String id)? pairRequested,
    TResult Function(String name, String id, String folder)?
        folderPairRequested,
    TResult Function(String name, String id)? devicePaired,
    TResult Function(String path, String device)? filePulled,
    TResult Function(String path, String device)? filePushed,
    TResult Function(String path, String winner, String loser)? conflict,
    TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
    required TResult orElse(),
  }) {
    if (error != null) {
      return error(message);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(SyncEvent_Syncing value) syncing,
    required TResult Function(SyncEvent_Idle value) idle,
    required TResult Function(SyncEvent_Error value) error,
    required TResult Function(SyncEvent_PairRequested value) pairRequested,
    required TResult Function(SyncEvent_FolderPairRequested value)
        folderPairRequested,
    required TResult Function(SyncEvent_DevicePaired value) devicePaired,
    required TResult Function(SyncEvent_FilePulled value) filePulled,
    required TResult Function(SyncEvent_FilePushed value) filePushed,
    required TResult Function(SyncEvent_Conflict value) conflict,
    required TResult Function(SyncEvent_Progress value) progress,
  }) {
    return error(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SyncEvent_Syncing value)? syncing,
    TResult? Function(SyncEvent_Idle value)? idle,
    TResult? Function(SyncEvent_Error value)? error,
    TResult? Function(SyncEvent_PairRequested value)? pairRequested,
    TResult? Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult? Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult? Function(SyncEvent_FilePulled value)? filePulled,
    TResult? Function(SyncEvent_FilePushed value)? filePushed,
    TResult? Function(SyncEvent_Conflict value)? conflict,
    TResult? Function(SyncEvent_Progress value)? progress,
  }) {
    return error?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SyncEvent_Syncing value)? syncing,
    TResult Function(SyncEvent_Idle value)? idle,
    TResult Function(SyncEvent_Error value)? error,
    TResult Function(SyncEvent_PairRequested value)? pairRequested,
    TResult Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult Function(SyncEvent_FilePulled value)? filePulled,
    TResult Function(SyncEvent_FilePushed value)? filePushed,
    TResult Function(SyncEvent_Conflict value)? conflict,
    TResult Function(SyncEvent_Progress value)? progress,
    required TResult orElse(),
  }) {
    if (error != null) {
      return error(this);
    }
    return orElse();
  }
}

abstract class SyncEvent_Error extends SyncEvent {
  const factory SyncEvent_Error({required final String message}) =
      _$SyncEvent_ErrorImpl;
  const SyncEvent_Error._() : super._();

  String get message;

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$SyncEvent_ErrorImplCopyWith<_$SyncEvent_ErrorImpl> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$SyncEvent_PairRequestedImplCopyWith<$Res> {
  factory _$$SyncEvent_PairRequestedImplCopyWith(
          _$SyncEvent_PairRequestedImpl value,
          $Res Function(_$SyncEvent_PairRequestedImpl) then) =
      __$$SyncEvent_PairRequestedImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String name, String id});
}

/// @nodoc
class __$$SyncEvent_PairRequestedImplCopyWithImpl<$Res>
    extends _$SyncEventCopyWithImpl<$Res, _$SyncEvent_PairRequestedImpl>
    implements _$$SyncEvent_PairRequestedImplCopyWith<$Res> {
  __$$SyncEvent_PairRequestedImplCopyWithImpl(
      _$SyncEvent_PairRequestedImpl _value,
      $Res Function(_$SyncEvent_PairRequestedImpl) _then)
      : super(_value, _then);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? name = null,
    Object? id = null,
  }) {
    return _then(_$SyncEvent_PairRequestedImpl(
      name: null == name
          ? _value.name
          : name // ignore: cast_nullable_to_non_nullable
              as String,
      id: null == id
          ? _value.id
          : id // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$SyncEvent_PairRequestedImpl extends SyncEvent_PairRequested {
  const _$SyncEvent_PairRequestedImpl({required this.name, required this.id})
      : super._();

  @override
  final String name;
  @override
  final String id;

  @override
  String toString() {
    return 'SyncEvent.pairRequested(name: $name, id: $id)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$SyncEvent_PairRequestedImpl &&
            (identical(other.name, name) || other.name == name) &&
            (identical(other.id, id) || other.id == id));
  }

  @override
  int get hashCode => Object.hash(runtimeType, name, id);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$SyncEvent_PairRequestedImplCopyWith<_$SyncEvent_PairRequestedImpl>
      get copyWith => __$$SyncEvent_PairRequestedImplCopyWithImpl<
          _$SyncEvent_PairRequestedImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String folderId) syncing,
    required TResult Function() idle,
    required TResult Function(String message) error,
    required TResult Function(String name, String id) pairRequested,
    required TResult Function(String name, String id, String folder)
        folderPairRequested,
    required TResult Function(String name, String id) devicePaired,
    required TResult Function(String path, String device) filePulled,
    required TResult Function(String path, String device) filePushed,
    required TResult Function(String path, String winner, String loser)
        conflict,
    required TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)
        progress,
  }) {
    return pairRequested(name, id);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String folderId)? syncing,
    TResult? Function()? idle,
    TResult? Function(String message)? error,
    TResult? Function(String name, String id)? pairRequested,
    TResult? Function(String name, String id, String folder)?
        folderPairRequested,
    TResult? Function(String name, String id)? devicePaired,
    TResult? Function(String path, String device)? filePulled,
    TResult? Function(String path, String device)? filePushed,
    TResult? Function(String path, String winner, String loser)? conflict,
    TResult? Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
  }) {
    return pairRequested?.call(name, id);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String folderId)? syncing,
    TResult Function()? idle,
    TResult Function(String message)? error,
    TResult Function(String name, String id)? pairRequested,
    TResult Function(String name, String id, String folder)?
        folderPairRequested,
    TResult Function(String name, String id)? devicePaired,
    TResult Function(String path, String device)? filePulled,
    TResult Function(String path, String device)? filePushed,
    TResult Function(String path, String winner, String loser)? conflict,
    TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
    required TResult orElse(),
  }) {
    if (pairRequested != null) {
      return pairRequested(name, id);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(SyncEvent_Syncing value) syncing,
    required TResult Function(SyncEvent_Idle value) idle,
    required TResult Function(SyncEvent_Error value) error,
    required TResult Function(SyncEvent_PairRequested value) pairRequested,
    required TResult Function(SyncEvent_FolderPairRequested value)
        folderPairRequested,
    required TResult Function(SyncEvent_DevicePaired value) devicePaired,
    required TResult Function(SyncEvent_FilePulled value) filePulled,
    required TResult Function(SyncEvent_FilePushed value) filePushed,
    required TResult Function(SyncEvent_Conflict value) conflict,
    required TResult Function(SyncEvent_Progress value) progress,
  }) {
    return pairRequested(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SyncEvent_Syncing value)? syncing,
    TResult? Function(SyncEvent_Idle value)? idle,
    TResult? Function(SyncEvent_Error value)? error,
    TResult? Function(SyncEvent_PairRequested value)? pairRequested,
    TResult? Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult? Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult? Function(SyncEvent_FilePulled value)? filePulled,
    TResult? Function(SyncEvent_FilePushed value)? filePushed,
    TResult? Function(SyncEvent_Conflict value)? conflict,
    TResult? Function(SyncEvent_Progress value)? progress,
  }) {
    return pairRequested?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SyncEvent_Syncing value)? syncing,
    TResult Function(SyncEvent_Idle value)? idle,
    TResult Function(SyncEvent_Error value)? error,
    TResult Function(SyncEvent_PairRequested value)? pairRequested,
    TResult Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult Function(SyncEvent_FilePulled value)? filePulled,
    TResult Function(SyncEvent_FilePushed value)? filePushed,
    TResult Function(SyncEvent_Conflict value)? conflict,
    TResult Function(SyncEvent_Progress value)? progress,
    required TResult orElse(),
  }) {
    if (pairRequested != null) {
      return pairRequested(this);
    }
    return orElse();
  }
}

abstract class SyncEvent_PairRequested extends SyncEvent {
  const factory SyncEvent_PairRequested(
      {required final String name,
      required final String id}) = _$SyncEvent_PairRequestedImpl;
  const SyncEvent_PairRequested._() : super._();

  String get name;
  String get id;

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$SyncEvent_PairRequestedImplCopyWith<_$SyncEvent_PairRequestedImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$SyncEvent_FolderPairRequestedImplCopyWith<$Res> {
  factory _$$SyncEvent_FolderPairRequestedImplCopyWith(
          _$SyncEvent_FolderPairRequestedImpl value,
          $Res Function(_$SyncEvent_FolderPairRequestedImpl) then) =
      __$$SyncEvent_FolderPairRequestedImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String name, String id, String folder});
}

/// @nodoc
class __$$SyncEvent_FolderPairRequestedImplCopyWithImpl<$Res>
    extends _$SyncEventCopyWithImpl<$Res, _$SyncEvent_FolderPairRequestedImpl>
    implements _$$SyncEvent_FolderPairRequestedImplCopyWith<$Res> {
  __$$SyncEvent_FolderPairRequestedImplCopyWithImpl(
      _$SyncEvent_FolderPairRequestedImpl _value,
      $Res Function(_$SyncEvent_FolderPairRequestedImpl) _then)
      : super(_value, _then);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? name = null,
    Object? id = null,
    Object? folder = null,
  }) {
    return _then(_$SyncEvent_FolderPairRequestedImpl(
      name: null == name
          ? _value.name
          : name // ignore: cast_nullable_to_non_nullable
              as String,
      id: null == id
          ? _value.id
          : id // ignore: cast_nullable_to_non_nullable
              as String,
      folder: null == folder
          ? _value.folder
          : folder // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$SyncEvent_FolderPairRequestedImpl
    extends SyncEvent_FolderPairRequested {
  const _$SyncEvent_FolderPairRequestedImpl(
      {required this.name, required this.id, required this.folder})
      : super._();

  @override
  final String name;
  @override
  final String id;
  @override
  final String folder;

  @override
  String toString() {
    return 'SyncEvent.folderPairRequested(name: $name, id: $id, folder: $folder)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$SyncEvent_FolderPairRequestedImpl &&
            (identical(other.name, name) || other.name == name) &&
            (identical(other.id, id) || other.id == id) &&
            (identical(other.folder, folder) || other.folder == folder));
  }

  @override
  int get hashCode => Object.hash(runtimeType, name, id, folder);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$SyncEvent_FolderPairRequestedImplCopyWith<
          _$SyncEvent_FolderPairRequestedImpl>
      get copyWith => __$$SyncEvent_FolderPairRequestedImplCopyWithImpl<
          _$SyncEvent_FolderPairRequestedImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String folderId) syncing,
    required TResult Function() idle,
    required TResult Function(String message) error,
    required TResult Function(String name, String id) pairRequested,
    required TResult Function(String name, String id, String folder)
        folderPairRequested,
    required TResult Function(String name, String id) devicePaired,
    required TResult Function(String path, String device) filePulled,
    required TResult Function(String path, String device) filePushed,
    required TResult Function(String path, String winner, String loser)
        conflict,
    required TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)
        progress,
  }) {
    return folderPairRequested(name, id, folder);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String folderId)? syncing,
    TResult? Function()? idle,
    TResult? Function(String message)? error,
    TResult? Function(String name, String id)? pairRequested,
    TResult? Function(String name, String id, String folder)?
        folderPairRequested,
    TResult? Function(String name, String id)? devicePaired,
    TResult? Function(String path, String device)? filePulled,
    TResult? Function(String path, String device)? filePushed,
    TResult? Function(String path, String winner, String loser)? conflict,
    TResult? Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
  }) {
    return folderPairRequested?.call(name, id, folder);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String folderId)? syncing,
    TResult Function()? idle,
    TResult Function(String message)? error,
    TResult Function(String name, String id)? pairRequested,
    TResult Function(String name, String id, String folder)?
        folderPairRequested,
    TResult Function(String name, String id)? devicePaired,
    TResult Function(String path, String device)? filePulled,
    TResult Function(String path, String device)? filePushed,
    TResult Function(String path, String winner, String loser)? conflict,
    TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
    required TResult orElse(),
  }) {
    if (folderPairRequested != null) {
      return folderPairRequested(name, id, folder);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(SyncEvent_Syncing value) syncing,
    required TResult Function(SyncEvent_Idle value) idle,
    required TResult Function(SyncEvent_Error value) error,
    required TResult Function(SyncEvent_PairRequested value) pairRequested,
    required TResult Function(SyncEvent_FolderPairRequested value)
        folderPairRequested,
    required TResult Function(SyncEvent_DevicePaired value) devicePaired,
    required TResult Function(SyncEvent_FilePulled value) filePulled,
    required TResult Function(SyncEvent_FilePushed value) filePushed,
    required TResult Function(SyncEvent_Conflict value) conflict,
    required TResult Function(SyncEvent_Progress value) progress,
  }) {
    return folderPairRequested(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SyncEvent_Syncing value)? syncing,
    TResult? Function(SyncEvent_Idle value)? idle,
    TResult? Function(SyncEvent_Error value)? error,
    TResult? Function(SyncEvent_PairRequested value)? pairRequested,
    TResult? Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult? Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult? Function(SyncEvent_FilePulled value)? filePulled,
    TResult? Function(SyncEvent_FilePushed value)? filePushed,
    TResult? Function(SyncEvent_Conflict value)? conflict,
    TResult? Function(SyncEvent_Progress value)? progress,
  }) {
    return folderPairRequested?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SyncEvent_Syncing value)? syncing,
    TResult Function(SyncEvent_Idle value)? idle,
    TResult Function(SyncEvent_Error value)? error,
    TResult Function(SyncEvent_PairRequested value)? pairRequested,
    TResult Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult Function(SyncEvent_FilePulled value)? filePulled,
    TResult Function(SyncEvent_FilePushed value)? filePushed,
    TResult Function(SyncEvent_Conflict value)? conflict,
    TResult Function(SyncEvent_Progress value)? progress,
    required TResult orElse(),
  }) {
    if (folderPairRequested != null) {
      return folderPairRequested(this);
    }
    return orElse();
  }
}

abstract class SyncEvent_FolderPairRequested extends SyncEvent {
  const factory SyncEvent_FolderPairRequested(
      {required final String name,
      required final String id,
      required final String folder}) = _$SyncEvent_FolderPairRequestedImpl;
  const SyncEvent_FolderPairRequested._() : super._();

  String get name;
  String get id;
  String get folder;

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$SyncEvent_FolderPairRequestedImplCopyWith<
          _$SyncEvent_FolderPairRequestedImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$SyncEvent_DevicePairedImplCopyWith<$Res> {
  factory _$$SyncEvent_DevicePairedImplCopyWith(
          _$SyncEvent_DevicePairedImpl value,
          $Res Function(_$SyncEvent_DevicePairedImpl) then) =
      __$$SyncEvent_DevicePairedImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String name, String id});
}

/// @nodoc
class __$$SyncEvent_DevicePairedImplCopyWithImpl<$Res>
    extends _$SyncEventCopyWithImpl<$Res, _$SyncEvent_DevicePairedImpl>
    implements _$$SyncEvent_DevicePairedImplCopyWith<$Res> {
  __$$SyncEvent_DevicePairedImplCopyWithImpl(
      _$SyncEvent_DevicePairedImpl _value,
      $Res Function(_$SyncEvent_DevicePairedImpl) _then)
      : super(_value, _then);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? name = null,
    Object? id = null,
  }) {
    return _then(_$SyncEvent_DevicePairedImpl(
      name: null == name
          ? _value.name
          : name // ignore: cast_nullable_to_non_nullable
              as String,
      id: null == id
          ? _value.id
          : id // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$SyncEvent_DevicePairedImpl extends SyncEvent_DevicePaired {
  const _$SyncEvent_DevicePairedImpl({required this.name, required this.id})
      : super._();

  @override
  final String name;
  @override
  final String id;

  @override
  String toString() {
    return 'SyncEvent.devicePaired(name: $name, id: $id)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$SyncEvent_DevicePairedImpl &&
            (identical(other.name, name) || other.name == name) &&
            (identical(other.id, id) || other.id == id));
  }

  @override
  int get hashCode => Object.hash(runtimeType, name, id);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$SyncEvent_DevicePairedImplCopyWith<_$SyncEvent_DevicePairedImpl>
      get copyWith => __$$SyncEvent_DevicePairedImplCopyWithImpl<
          _$SyncEvent_DevicePairedImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String folderId) syncing,
    required TResult Function() idle,
    required TResult Function(String message) error,
    required TResult Function(String name, String id) pairRequested,
    required TResult Function(String name, String id, String folder)
        folderPairRequested,
    required TResult Function(String name, String id) devicePaired,
    required TResult Function(String path, String device) filePulled,
    required TResult Function(String path, String device) filePushed,
    required TResult Function(String path, String winner, String loser)
        conflict,
    required TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)
        progress,
  }) {
    return devicePaired(name, id);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String folderId)? syncing,
    TResult? Function()? idle,
    TResult? Function(String message)? error,
    TResult? Function(String name, String id)? pairRequested,
    TResult? Function(String name, String id, String folder)?
        folderPairRequested,
    TResult? Function(String name, String id)? devicePaired,
    TResult? Function(String path, String device)? filePulled,
    TResult? Function(String path, String device)? filePushed,
    TResult? Function(String path, String winner, String loser)? conflict,
    TResult? Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
  }) {
    return devicePaired?.call(name, id);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String folderId)? syncing,
    TResult Function()? idle,
    TResult Function(String message)? error,
    TResult Function(String name, String id)? pairRequested,
    TResult Function(String name, String id, String folder)?
        folderPairRequested,
    TResult Function(String name, String id)? devicePaired,
    TResult Function(String path, String device)? filePulled,
    TResult Function(String path, String device)? filePushed,
    TResult Function(String path, String winner, String loser)? conflict,
    TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
    required TResult orElse(),
  }) {
    if (devicePaired != null) {
      return devicePaired(name, id);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(SyncEvent_Syncing value) syncing,
    required TResult Function(SyncEvent_Idle value) idle,
    required TResult Function(SyncEvent_Error value) error,
    required TResult Function(SyncEvent_PairRequested value) pairRequested,
    required TResult Function(SyncEvent_FolderPairRequested value)
        folderPairRequested,
    required TResult Function(SyncEvent_DevicePaired value) devicePaired,
    required TResult Function(SyncEvent_FilePulled value) filePulled,
    required TResult Function(SyncEvent_FilePushed value) filePushed,
    required TResult Function(SyncEvent_Conflict value) conflict,
    required TResult Function(SyncEvent_Progress value) progress,
  }) {
    return devicePaired(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SyncEvent_Syncing value)? syncing,
    TResult? Function(SyncEvent_Idle value)? idle,
    TResult? Function(SyncEvent_Error value)? error,
    TResult? Function(SyncEvent_PairRequested value)? pairRequested,
    TResult? Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult? Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult? Function(SyncEvent_FilePulled value)? filePulled,
    TResult? Function(SyncEvent_FilePushed value)? filePushed,
    TResult? Function(SyncEvent_Conflict value)? conflict,
    TResult? Function(SyncEvent_Progress value)? progress,
  }) {
    return devicePaired?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SyncEvent_Syncing value)? syncing,
    TResult Function(SyncEvent_Idle value)? idle,
    TResult Function(SyncEvent_Error value)? error,
    TResult Function(SyncEvent_PairRequested value)? pairRequested,
    TResult Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult Function(SyncEvent_FilePulled value)? filePulled,
    TResult Function(SyncEvent_FilePushed value)? filePushed,
    TResult Function(SyncEvent_Conflict value)? conflict,
    TResult Function(SyncEvent_Progress value)? progress,
    required TResult orElse(),
  }) {
    if (devicePaired != null) {
      return devicePaired(this);
    }
    return orElse();
  }
}

abstract class SyncEvent_DevicePaired extends SyncEvent {
  const factory SyncEvent_DevicePaired(
      {required final String name,
      required final String id}) = _$SyncEvent_DevicePairedImpl;
  const SyncEvent_DevicePaired._() : super._();

  String get name;
  String get id;

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$SyncEvent_DevicePairedImplCopyWith<_$SyncEvent_DevicePairedImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$SyncEvent_FilePulledImplCopyWith<$Res> {
  factory _$$SyncEvent_FilePulledImplCopyWith(_$SyncEvent_FilePulledImpl value,
          $Res Function(_$SyncEvent_FilePulledImpl) then) =
      __$$SyncEvent_FilePulledImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String path, String device});
}

/// @nodoc
class __$$SyncEvent_FilePulledImplCopyWithImpl<$Res>
    extends _$SyncEventCopyWithImpl<$Res, _$SyncEvent_FilePulledImpl>
    implements _$$SyncEvent_FilePulledImplCopyWith<$Res> {
  __$$SyncEvent_FilePulledImplCopyWithImpl(_$SyncEvent_FilePulledImpl _value,
      $Res Function(_$SyncEvent_FilePulledImpl) _then)
      : super(_value, _then);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? path = null,
    Object? device = null,
  }) {
    return _then(_$SyncEvent_FilePulledImpl(
      path: null == path
          ? _value.path
          : path // ignore: cast_nullable_to_non_nullable
              as String,
      device: null == device
          ? _value.device
          : device // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$SyncEvent_FilePulledImpl extends SyncEvent_FilePulled {
  const _$SyncEvent_FilePulledImpl({required this.path, required this.device})
      : super._();

  @override
  final String path;
  @override
  final String device;

  @override
  String toString() {
    return 'SyncEvent.filePulled(path: $path, device: $device)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$SyncEvent_FilePulledImpl &&
            (identical(other.path, path) || other.path == path) &&
            (identical(other.device, device) || other.device == device));
  }

  @override
  int get hashCode => Object.hash(runtimeType, path, device);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$SyncEvent_FilePulledImplCopyWith<_$SyncEvent_FilePulledImpl>
      get copyWith =>
          __$$SyncEvent_FilePulledImplCopyWithImpl<_$SyncEvent_FilePulledImpl>(
              this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String folderId) syncing,
    required TResult Function() idle,
    required TResult Function(String message) error,
    required TResult Function(String name, String id) pairRequested,
    required TResult Function(String name, String id, String folder)
        folderPairRequested,
    required TResult Function(String name, String id) devicePaired,
    required TResult Function(String path, String device) filePulled,
    required TResult Function(String path, String device) filePushed,
    required TResult Function(String path, String winner, String loser)
        conflict,
    required TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)
        progress,
  }) {
    return filePulled(path, device);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String folderId)? syncing,
    TResult? Function()? idle,
    TResult? Function(String message)? error,
    TResult? Function(String name, String id)? pairRequested,
    TResult? Function(String name, String id, String folder)?
        folderPairRequested,
    TResult? Function(String name, String id)? devicePaired,
    TResult? Function(String path, String device)? filePulled,
    TResult? Function(String path, String device)? filePushed,
    TResult? Function(String path, String winner, String loser)? conflict,
    TResult? Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
  }) {
    return filePulled?.call(path, device);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String folderId)? syncing,
    TResult Function()? idle,
    TResult Function(String message)? error,
    TResult Function(String name, String id)? pairRequested,
    TResult Function(String name, String id, String folder)?
        folderPairRequested,
    TResult Function(String name, String id)? devicePaired,
    TResult Function(String path, String device)? filePulled,
    TResult Function(String path, String device)? filePushed,
    TResult Function(String path, String winner, String loser)? conflict,
    TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
    required TResult orElse(),
  }) {
    if (filePulled != null) {
      return filePulled(path, device);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(SyncEvent_Syncing value) syncing,
    required TResult Function(SyncEvent_Idle value) idle,
    required TResult Function(SyncEvent_Error value) error,
    required TResult Function(SyncEvent_PairRequested value) pairRequested,
    required TResult Function(SyncEvent_FolderPairRequested value)
        folderPairRequested,
    required TResult Function(SyncEvent_DevicePaired value) devicePaired,
    required TResult Function(SyncEvent_FilePulled value) filePulled,
    required TResult Function(SyncEvent_FilePushed value) filePushed,
    required TResult Function(SyncEvent_Conflict value) conflict,
    required TResult Function(SyncEvent_Progress value) progress,
  }) {
    return filePulled(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SyncEvent_Syncing value)? syncing,
    TResult? Function(SyncEvent_Idle value)? idle,
    TResult? Function(SyncEvent_Error value)? error,
    TResult? Function(SyncEvent_PairRequested value)? pairRequested,
    TResult? Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult? Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult? Function(SyncEvent_FilePulled value)? filePulled,
    TResult? Function(SyncEvent_FilePushed value)? filePushed,
    TResult? Function(SyncEvent_Conflict value)? conflict,
    TResult? Function(SyncEvent_Progress value)? progress,
  }) {
    return filePulled?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SyncEvent_Syncing value)? syncing,
    TResult Function(SyncEvent_Idle value)? idle,
    TResult Function(SyncEvent_Error value)? error,
    TResult Function(SyncEvent_PairRequested value)? pairRequested,
    TResult Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult Function(SyncEvent_FilePulled value)? filePulled,
    TResult Function(SyncEvent_FilePushed value)? filePushed,
    TResult Function(SyncEvent_Conflict value)? conflict,
    TResult Function(SyncEvent_Progress value)? progress,
    required TResult orElse(),
  }) {
    if (filePulled != null) {
      return filePulled(this);
    }
    return orElse();
  }
}

abstract class SyncEvent_FilePulled extends SyncEvent {
  const factory SyncEvent_FilePulled(
      {required final String path,
      required final String device}) = _$SyncEvent_FilePulledImpl;
  const SyncEvent_FilePulled._() : super._();

  String get path;
  String get device;

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$SyncEvent_FilePulledImplCopyWith<_$SyncEvent_FilePulledImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$SyncEvent_FilePushedImplCopyWith<$Res> {
  factory _$$SyncEvent_FilePushedImplCopyWith(_$SyncEvent_FilePushedImpl value,
          $Res Function(_$SyncEvent_FilePushedImpl) then) =
      __$$SyncEvent_FilePushedImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String path, String device});
}

/// @nodoc
class __$$SyncEvent_FilePushedImplCopyWithImpl<$Res>
    extends _$SyncEventCopyWithImpl<$Res, _$SyncEvent_FilePushedImpl>
    implements _$$SyncEvent_FilePushedImplCopyWith<$Res> {
  __$$SyncEvent_FilePushedImplCopyWithImpl(_$SyncEvent_FilePushedImpl _value,
      $Res Function(_$SyncEvent_FilePushedImpl) _then)
      : super(_value, _then);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? path = null,
    Object? device = null,
  }) {
    return _then(_$SyncEvent_FilePushedImpl(
      path: null == path
          ? _value.path
          : path // ignore: cast_nullable_to_non_nullable
              as String,
      device: null == device
          ? _value.device
          : device // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$SyncEvent_FilePushedImpl extends SyncEvent_FilePushed {
  const _$SyncEvent_FilePushedImpl({required this.path, required this.device})
      : super._();

  @override
  final String path;
  @override
  final String device;

  @override
  String toString() {
    return 'SyncEvent.filePushed(path: $path, device: $device)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$SyncEvent_FilePushedImpl &&
            (identical(other.path, path) || other.path == path) &&
            (identical(other.device, device) || other.device == device));
  }

  @override
  int get hashCode => Object.hash(runtimeType, path, device);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$SyncEvent_FilePushedImplCopyWith<_$SyncEvent_FilePushedImpl>
      get copyWith =>
          __$$SyncEvent_FilePushedImplCopyWithImpl<_$SyncEvent_FilePushedImpl>(
              this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String folderId) syncing,
    required TResult Function() idle,
    required TResult Function(String message) error,
    required TResult Function(String name, String id) pairRequested,
    required TResult Function(String name, String id, String folder)
        folderPairRequested,
    required TResult Function(String name, String id) devicePaired,
    required TResult Function(String path, String device) filePulled,
    required TResult Function(String path, String device) filePushed,
    required TResult Function(String path, String winner, String loser)
        conflict,
    required TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)
        progress,
  }) {
    return filePushed(path, device);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String folderId)? syncing,
    TResult? Function()? idle,
    TResult? Function(String message)? error,
    TResult? Function(String name, String id)? pairRequested,
    TResult? Function(String name, String id, String folder)?
        folderPairRequested,
    TResult? Function(String name, String id)? devicePaired,
    TResult? Function(String path, String device)? filePulled,
    TResult? Function(String path, String device)? filePushed,
    TResult? Function(String path, String winner, String loser)? conflict,
    TResult? Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
  }) {
    return filePushed?.call(path, device);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String folderId)? syncing,
    TResult Function()? idle,
    TResult Function(String message)? error,
    TResult Function(String name, String id)? pairRequested,
    TResult Function(String name, String id, String folder)?
        folderPairRequested,
    TResult Function(String name, String id)? devicePaired,
    TResult Function(String path, String device)? filePulled,
    TResult Function(String path, String device)? filePushed,
    TResult Function(String path, String winner, String loser)? conflict,
    TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
    required TResult orElse(),
  }) {
    if (filePushed != null) {
      return filePushed(path, device);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(SyncEvent_Syncing value) syncing,
    required TResult Function(SyncEvent_Idle value) idle,
    required TResult Function(SyncEvent_Error value) error,
    required TResult Function(SyncEvent_PairRequested value) pairRequested,
    required TResult Function(SyncEvent_FolderPairRequested value)
        folderPairRequested,
    required TResult Function(SyncEvent_DevicePaired value) devicePaired,
    required TResult Function(SyncEvent_FilePulled value) filePulled,
    required TResult Function(SyncEvent_FilePushed value) filePushed,
    required TResult Function(SyncEvent_Conflict value) conflict,
    required TResult Function(SyncEvent_Progress value) progress,
  }) {
    return filePushed(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SyncEvent_Syncing value)? syncing,
    TResult? Function(SyncEvent_Idle value)? idle,
    TResult? Function(SyncEvent_Error value)? error,
    TResult? Function(SyncEvent_PairRequested value)? pairRequested,
    TResult? Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult? Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult? Function(SyncEvent_FilePulled value)? filePulled,
    TResult? Function(SyncEvent_FilePushed value)? filePushed,
    TResult? Function(SyncEvent_Conflict value)? conflict,
    TResult? Function(SyncEvent_Progress value)? progress,
  }) {
    return filePushed?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SyncEvent_Syncing value)? syncing,
    TResult Function(SyncEvent_Idle value)? idle,
    TResult Function(SyncEvent_Error value)? error,
    TResult Function(SyncEvent_PairRequested value)? pairRequested,
    TResult Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult Function(SyncEvent_FilePulled value)? filePulled,
    TResult Function(SyncEvent_FilePushed value)? filePushed,
    TResult Function(SyncEvent_Conflict value)? conflict,
    TResult Function(SyncEvent_Progress value)? progress,
    required TResult orElse(),
  }) {
    if (filePushed != null) {
      return filePushed(this);
    }
    return orElse();
  }
}

abstract class SyncEvent_FilePushed extends SyncEvent {
  const factory SyncEvent_FilePushed(
      {required final String path,
      required final String device}) = _$SyncEvent_FilePushedImpl;
  const SyncEvent_FilePushed._() : super._();

  String get path;
  String get device;

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$SyncEvent_FilePushedImplCopyWith<_$SyncEvent_FilePushedImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$SyncEvent_ConflictImplCopyWith<$Res> {
  factory _$$SyncEvent_ConflictImplCopyWith(_$SyncEvent_ConflictImpl value,
          $Res Function(_$SyncEvent_ConflictImpl) then) =
      __$$SyncEvent_ConflictImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String path, String winner, String loser});
}

/// @nodoc
class __$$SyncEvent_ConflictImplCopyWithImpl<$Res>
    extends _$SyncEventCopyWithImpl<$Res, _$SyncEvent_ConflictImpl>
    implements _$$SyncEvent_ConflictImplCopyWith<$Res> {
  __$$SyncEvent_ConflictImplCopyWithImpl(_$SyncEvent_ConflictImpl _value,
      $Res Function(_$SyncEvent_ConflictImpl) _then)
      : super(_value, _then);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? path = null,
    Object? winner = null,
    Object? loser = null,
  }) {
    return _then(_$SyncEvent_ConflictImpl(
      path: null == path
          ? _value.path
          : path // ignore: cast_nullable_to_non_nullable
              as String,
      winner: null == winner
          ? _value.winner
          : winner // ignore: cast_nullable_to_non_nullable
              as String,
      loser: null == loser
          ? _value.loser
          : loser // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$SyncEvent_ConflictImpl extends SyncEvent_Conflict {
  const _$SyncEvent_ConflictImpl(
      {required this.path, required this.winner, required this.loser})
      : super._();

  @override
  final String path;
  @override
  final String winner;
  @override
  final String loser;

  @override
  String toString() {
    return 'SyncEvent.conflict(path: $path, winner: $winner, loser: $loser)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$SyncEvent_ConflictImpl &&
            (identical(other.path, path) || other.path == path) &&
            (identical(other.winner, winner) || other.winner == winner) &&
            (identical(other.loser, loser) || other.loser == loser));
  }

  @override
  int get hashCode => Object.hash(runtimeType, path, winner, loser);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$SyncEvent_ConflictImplCopyWith<_$SyncEvent_ConflictImpl> get copyWith =>
      __$$SyncEvent_ConflictImplCopyWithImpl<_$SyncEvent_ConflictImpl>(
          this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String folderId) syncing,
    required TResult Function() idle,
    required TResult Function(String message) error,
    required TResult Function(String name, String id) pairRequested,
    required TResult Function(String name, String id, String folder)
        folderPairRequested,
    required TResult Function(String name, String id) devicePaired,
    required TResult Function(String path, String device) filePulled,
    required TResult Function(String path, String device) filePushed,
    required TResult Function(String path, String winner, String loser)
        conflict,
    required TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)
        progress,
  }) {
    return conflict(path, winner, loser);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String folderId)? syncing,
    TResult? Function()? idle,
    TResult? Function(String message)? error,
    TResult? Function(String name, String id)? pairRequested,
    TResult? Function(String name, String id, String folder)?
        folderPairRequested,
    TResult? Function(String name, String id)? devicePaired,
    TResult? Function(String path, String device)? filePulled,
    TResult? Function(String path, String device)? filePushed,
    TResult? Function(String path, String winner, String loser)? conflict,
    TResult? Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
  }) {
    return conflict?.call(path, winner, loser);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String folderId)? syncing,
    TResult Function()? idle,
    TResult Function(String message)? error,
    TResult Function(String name, String id)? pairRequested,
    TResult Function(String name, String id, String folder)?
        folderPairRequested,
    TResult Function(String name, String id)? devicePaired,
    TResult Function(String path, String device)? filePulled,
    TResult Function(String path, String device)? filePushed,
    TResult Function(String path, String winner, String loser)? conflict,
    TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
    required TResult orElse(),
  }) {
    if (conflict != null) {
      return conflict(path, winner, loser);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(SyncEvent_Syncing value) syncing,
    required TResult Function(SyncEvent_Idle value) idle,
    required TResult Function(SyncEvent_Error value) error,
    required TResult Function(SyncEvent_PairRequested value) pairRequested,
    required TResult Function(SyncEvent_FolderPairRequested value)
        folderPairRequested,
    required TResult Function(SyncEvent_DevicePaired value) devicePaired,
    required TResult Function(SyncEvent_FilePulled value) filePulled,
    required TResult Function(SyncEvent_FilePushed value) filePushed,
    required TResult Function(SyncEvent_Conflict value) conflict,
    required TResult Function(SyncEvent_Progress value) progress,
  }) {
    return conflict(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SyncEvent_Syncing value)? syncing,
    TResult? Function(SyncEvent_Idle value)? idle,
    TResult? Function(SyncEvent_Error value)? error,
    TResult? Function(SyncEvent_PairRequested value)? pairRequested,
    TResult? Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult? Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult? Function(SyncEvent_FilePulled value)? filePulled,
    TResult? Function(SyncEvent_FilePushed value)? filePushed,
    TResult? Function(SyncEvent_Conflict value)? conflict,
    TResult? Function(SyncEvent_Progress value)? progress,
  }) {
    return conflict?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SyncEvent_Syncing value)? syncing,
    TResult Function(SyncEvent_Idle value)? idle,
    TResult Function(SyncEvent_Error value)? error,
    TResult Function(SyncEvent_PairRequested value)? pairRequested,
    TResult Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult Function(SyncEvent_FilePulled value)? filePulled,
    TResult Function(SyncEvent_FilePushed value)? filePushed,
    TResult Function(SyncEvent_Conflict value)? conflict,
    TResult Function(SyncEvent_Progress value)? progress,
    required TResult orElse(),
  }) {
    if (conflict != null) {
      return conflict(this);
    }
    return orElse();
  }
}

abstract class SyncEvent_Conflict extends SyncEvent {
  const factory SyncEvent_Conflict(
      {required final String path,
      required final String winner,
      required final String loser}) = _$SyncEvent_ConflictImpl;
  const SyncEvent_Conflict._() : super._();

  String get path;
  String get winner;
  String get loser;

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$SyncEvent_ConflictImplCopyWith<_$SyncEvent_ConflictImpl> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$SyncEvent_ProgressImplCopyWith<$Res> {
  factory _$$SyncEvent_ProgressImplCopyWith(_$SyncEvent_ProgressImpl value,
          $Res Function(_$SyncEvent_ProgressImpl) then) =
      __$$SyncEvent_ProgressImplCopyWithImpl<$Res>;
  @useResult
  $Res call(
      {String folderId,
      String stage,
      BigInt filesDone,
      BigInt filesTotal,
      BigInt bytesDone,
      BigInt bytesTotal});
}

/// @nodoc
class __$$SyncEvent_ProgressImplCopyWithImpl<$Res>
    extends _$SyncEventCopyWithImpl<$Res, _$SyncEvent_ProgressImpl>
    implements _$$SyncEvent_ProgressImplCopyWith<$Res> {
  __$$SyncEvent_ProgressImplCopyWithImpl(_$SyncEvent_ProgressImpl _value,
      $Res Function(_$SyncEvent_ProgressImpl) _then)
      : super(_value, _then);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? folderId = null,
    Object? stage = null,
    Object? filesDone = null,
    Object? filesTotal = null,
    Object? bytesDone = null,
    Object? bytesTotal = null,
  }) {
    return _then(_$SyncEvent_ProgressImpl(
      folderId: null == folderId
          ? _value.folderId
          : folderId // ignore: cast_nullable_to_non_nullable
              as String,
      stage: null == stage
          ? _value.stage
          : stage // ignore: cast_nullable_to_non_nullable
              as String,
      filesDone: null == filesDone
          ? _value.filesDone
          : filesDone // ignore: cast_nullable_to_non_nullable
              as BigInt,
      filesTotal: null == filesTotal
          ? _value.filesTotal
          : filesTotal // ignore: cast_nullable_to_non_nullable
              as BigInt,
      bytesDone: null == bytesDone
          ? _value.bytesDone
          : bytesDone // ignore: cast_nullable_to_non_nullable
              as BigInt,
      bytesTotal: null == bytesTotal
          ? _value.bytesTotal
          : bytesTotal // ignore: cast_nullable_to_non_nullable
              as BigInt,
    ));
  }
}

/// @nodoc

class _$SyncEvent_ProgressImpl extends SyncEvent_Progress {
  const _$SyncEvent_ProgressImpl(
      {required this.folderId,
      required this.stage,
      required this.filesDone,
      required this.filesTotal,
      required this.bytesDone,
      required this.bytesTotal})
      : super._();

  @override
  final String folderId;
  @override
  final String stage;
  @override
  final BigInt filesDone;
  @override
  final BigInt filesTotal;
  @override
  final BigInt bytesDone;
  @override
  final BigInt bytesTotal;

  @override
  String toString() {
    return 'SyncEvent.progress(folderId: $folderId, stage: $stage, filesDone: $filesDone, filesTotal: $filesTotal, bytesDone: $bytesDone, bytesTotal: $bytesTotal)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$SyncEvent_ProgressImpl &&
            (identical(other.folderId, folderId) ||
                other.folderId == folderId) &&
            (identical(other.stage, stage) || other.stage == stage) &&
            (identical(other.filesDone, filesDone) ||
                other.filesDone == filesDone) &&
            (identical(other.filesTotal, filesTotal) ||
                other.filesTotal == filesTotal) &&
            (identical(other.bytesDone, bytesDone) ||
                other.bytesDone == bytesDone) &&
            (identical(other.bytesTotal, bytesTotal) ||
                other.bytesTotal == bytesTotal));
  }

  @override
  int get hashCode => Object.hash(runtimeType, folderId, stage, filesDone,
      filesTotal, bytesDone, bytesTotal);

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$SyncEvent_ProgressImplCopyWith<_$SyncEvent_ProgressImpl> get copyWith =>
      __$$SyncEvent_ProgressImplCopyWithImpl<_$SyncEvent_ProgressImpl>(
          this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String folderId) syncing,
    required TResult Function() idle,
    required TResult Function(String message) error,
    required TResult Function(String name, String id) pairRequested,
    required TResult Function(String name, String id, String folder)
        folderPairRequested,
    required TResult Function(String name, String id) devicePaired,
    required TResult Function(String path, String device) filePulled,
    required TResult Function(String path, String device) filePushed,
    required TResult Function(String path, String winner, String loser)
        conflict,
    required TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)
        progress,
  }) {
    return progress(
        folderId, stage, filesDone, filesTotal, bytesDone, bytesTotal);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String folderId)? syncing,
    TResult? Function()? idle,
    TResult? Function(String message)? error,
    TResult? Function(String name, String id)? pairRequested,
    TResult? Function(String name, String id, String folder)?
        folderPairRequested,
    TResult? Function(String name, String id)? devicePaired,
    TResult? Function(String path, String device)? filePulled,
    TResult? Function(String path, String device)? filePushed,
    TResult? Function(String path, String winner, String loser)? conflict,
    TResult? Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
  }) {
    return progress?.call(
        folderId, stage, filesDone, filesTotal, bytesDone, bytesTotal);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String folderId)? syncing,
    TResult Function()? idle,
    TResult Function(String message)? error,
    TResult Function(String name, String id)? pairRequested,
    TResult Function(String name, String id, String folder)?
        folderPairRequested,
    TResult Function(String name, String id)? devicePaired,
    TResult Function(String path, String device)? filePulled,
    TResult Function(String path, String device)? filePushed,
    TResult Function(String path, String winner, String loser)? conflict,
    TResult Function(String folderId, String stage, BigInt filesDone,
            BigInt filesTotal, BigInt bytesDone, BigInt bytesTotal)?
        progress,
    required TResult orElse(),
  }) {
    if (progress != null) {
      return progress(
          folderId, stage, filesDone, filesTotal, bytesDone, bytesTotal);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(SyncEvent_Syncing value) syncing,
    required TResult Function(SyncEvent_Idle value) idle,
    required TResult Function(SyncEvent_Error value) error,
    required TResult Function(SyncEvent_PairRequested value) pairRequested,
    required TResult Function(SyncEvent_FolderPairRequested value)
        folderPairRequested,
    required TResult Function(SyncEvent_DevicePaired value) devicePaired,
    required TResult Function(SyncEvent_FilePulled value) filePulled,
    required TResult Function(SyncEvent_FilePushed value) filePushed,
    required TResult Function(SyncEvent_Conflict value) conflict,
    required TResult Function(SyncEvent_Progress value) progress,
  }) {
    return progress(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SyncEvent_Syncing value)? syncing,
    TResult? Function(SyncEvent_Idle value)? idle,
    TResult? Function(SyncEvent_Error value)? error,
    TResult? Function(SyncEvent_PairRequested value)? pairRequested,
    TResult? Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult? Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult? Function(SyncEvent_FilePulled value)? filePulled,
    TResult? Function(SyncEvent_FilePushed value)? filePushed,
    TResult? Function(SyncEvent_Conflict value)? conflict,
    TResult? Function(SyncEvent_Progress value)? progress,
  }) {
    return progress?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SyncEvent_Syncing value)? syncing,
    TResult Function(SyncEvent_Idle value)? idle,
    TResult Function(SyncEvent_Error value)? error,
    TResult Function(SyncEvent_PairRequested value)? pairRequested,
    TResult Function(SyncEvent_FolderPairRequested value)? folderPairRequested,
    TResult Function(SyncEvent_DevicePaired value)? devicePaired,
    TResult Function(SyncEvent_FilePulled value)? filePulled,
    TResult Function(SyncEvent_FilePushed value)? filePushed,
    TResult Function(SyncEvent_Conflict value)? conflict,
    TResult Function(SyncEvent_Progress value)? progress,
    required TResult orElse(),
  }) {
    if (progress != null) {
      return progress(this);
    }
    return orElse();
  }
}

abstract class SyncEvent_Progress extends SyncEvent {
  const factory SyncEvent_Progress(
      {required final String folderId,
      required final String stage,
      required final BigInt filesDone,
      required final BigInt filesTotal,
      required final BigInt bytesDone,
      required final BigInt bytesTotal}) = _$SyncEvent_ProgressImpl;
  const SyncEvent_Progress._() : super._();

  String get folderId;
  String get stage;
  BigInt get filesDone;
  BigInt get filesTotal;
  BigInt get bytesDone;
  BigInt get bytesTotal;

  /// Create a copy of SyncEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$SyncEvent_ProgressImplCopyWith<_$SyncEvent_ProgressImpl> get copyWith =>
      throw _privateConstructorUsedError;
}
