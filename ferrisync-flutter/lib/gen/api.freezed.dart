// coverage:ignore-file
// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'api.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

T _$identity<T>(T value) => value;

final _privateConstructorUsedError = UnsupportedError(
    'It seems like you constructed your class using `MyClass._()`. This constructor is only meant to be used by freezed and you are not supposed to need it nor use it.\nPlease check the documentation here for more information: https://github.com/rrousselGit/freezed#adding-getters-and-methods-to-our-models');

/// @nodoc
mixin _$BrowseSharedFoldersResult {
  Object get field0 => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(List<RemoteSharedFolder> field0) success,
    required TResult Function(PeerRequestError field0) failure,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(List<RemoteSharedFolder> field0)? success,
    TResult? Function(PeerRequestError field0)? failure,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(List<RemoteSharedFolder> field0)? success,
    TResult Function(PeerRequestError field0)? failure,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(BrowseSharedFoldersResult_Success value) success,
    required TResult Function(BrowseSharedFoldersResult_Failure value) failure,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(BrowseSharedFoldersResult_Success value)? success,
    TResult? Function(BrowseSharedFoldersResult_Failure value)? failure,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(BrowseSharedFoldersResult_Success value)? success,
    TResult Function(BrowseSharedFoldersResult_Failure value)? failure,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class $BrowseSharedFoldersResultCopyWith<$Res> {
  factory $BrowseSharedFoldersResultCopyWith(BrowseSharedFoldersResult value,
          $Res Function(BrowseSharedFoldersResult) then) =
      _$BrowseSharedFoldersResultCopyWithImpl<$Res, BrowseSharedFoldersResult>;
}

/// @nodoc
class _$BrowseSharedFoldersResultCopyWithImpl<$Res,
        $Val extends BrowseSharedFoldersResult>
    implements $BrowseSharedFoldersResultCopyWith<$Res> {
  _$BrowseSharedFoldersResultCopyWithImpl(this._value, this._then);

  // ignore: unused_field
  final $Val _value;
  // ignore: unused_field
  final $Res Function($Val) _then;

  /// Create a copy of BrowseSharedFoldersResult
  /// with the given fields replaced by the non-null parameter values.
}

/// @nodoc
abstract class _$$BrowseSharedFoldersResult_SuccessImplCopyWith<$Res> {
  factory _$$BrowseSharedFoldersResult_SuccessImplCopyWith(
          _$BrowseSharedFoldersResult_SuccessImpl value,
          $Res Function(_$BrowseSharedFoldersResult_SuccessImpl) then) =
      __$$BrowseSharedFoldersResult_SuccessImplCopyWithImpl<$Res>;
  @useResult
  $Res call({List<RemoteSharedFolder> field0});
}

/// @nodoc
class __$$BrowseSharedFoldersResult_SuccessImplCopyWithImpl<$Res>
    extends _$BrowseSharedFoldersResultCopyWithImpl<$Res,
        _$BrowseSharedFoldersResult_SuccessImpl>
    implements _$$BrowseSharedFoldersResult_SuccessImplCopyWith<$Res> {
  __$$BrowseSharedFoldersResult_SuccessImplCopyWithImpl(
      _$BrowseSharedFoldersResult_SuccessImpl _value,
      $Res Function(_$BrowseSharedFoldersResult_SuccessImpl) _then)
      : super(_value, _then);

  /// Create a copy of BrowseSharedFoldersResult
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$BrowseSharedFoldersResult_SuccessImpl(
      null == field0
          ? _value._field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as List<RemoteSharedFolder>,
    ));
  }
}

/// @nodoc

class _$BrowseSharedFoldersResult_SuccessImpl
    extends BrowseSharedFoldersResult_Success {
  const _$BrowseSharedFoldersResult_SuccessImpl(
      final List<RemoteSharedFolder> field0)
      : _field0 = field0,
        super._();

  final List<RemoteSharedFolder> _field0;
  @override
  List<RemoteSharedFolder> get field0 {
    if (_field0 is EqualUnmodifiableListView) return _field0;
    // ignore: implicit_dynamic_type
    return EqualUnmodifiableListView(_field0);
  }

  @override
  String toString() {
    return 'BrowseSharedFoldersResult.success(field0: $field0)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$BrowseSharedFoldersResult_SuccessImpl &&
            const DeepCollectionEquality().equals(other._field0, _field0));
  }

  @override
  int get hashCode =>
      Object.hash(runtimeType, const DeepCollectionEquality().hash(_field0));

  /// Create a copy of BrowseSharedFoldersResult
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$BrowseSharedFoldersResult_SuccessImplCopyWith<
          _$BrowseSharedFoldersResult_SuccessImpl>
      get copyWith => __$$BrowseSharedFoldersResult_SuccessImplCopyWithImpl<
          _$BrowseSharedFoldersResult_SuccessImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(List<RemoteSharedFolder> field0) success,
    required TResult Function(PeerRequestError field0) failure,
  }) {
    return success(field0);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(List<RemoteSharedFolder> field0)? success,
    TResult? Function(PeerRequestError field0)? failure,
  }) {
    return success?.call(field0);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(List<RemoteSharedFolder> field0)? success,
    TResult Function(PeerRequestError field0)? failure,
    required TResult orElse(),
  }) {
    if (success != null) {
      return success(field0);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(BrowseSharedFoldersResult_Success value) success,
    required TResult Function(BrowseSharedFoldersResult_Failure value) failure,
  }) {
    return success(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(BrowseSharedFoldersResult_Success value)? success,
    TResult? Function(BrowseSharedFoldersResult_Failure value)? failure,
  }) {
    return success?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(BrowseSharedFoldersResult_Success value)? success,
    TResult Function(BrowseSharedFoldersResult_Failure value)? failure,
    required TResult orElse(),
  }) {
    if (success != null) {
      return success(this);
    }
    return orElse();
  }
}

abstract class BrowseSharedFoldersResult_Success
    extends BrowseSharedFoldersResult {
  const factory BrowseSharedFoldersResult_Success(
          final List<RemoteSharedFolder> field0) =
      _$BrowseSharedFoldersResult_SuccessImpl;
  const BrowseSharedFoldersResult_Success._() : super._();

  @override
  List<RemoteSharedFolder> get field0;

  /// Create a copy of BrowseSharedFoldersResult
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$BrowseSharedFoldersResult_SuccessImplCopyWith<
          _$BrowseSharedFoldersResult_SuccessImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$BrowseSharedFoldersResult_FailureImplCopyWith<$Res> {
  factory _$$BrowseSharedFoldersResult_FailureImplCopyWith(
          _$BrowseSharedFoldersResult_FailureImpl value,
          $Res Function(_$BrowseSharedFoldersResult_FailureImpl) then) =
      __$$BrowseSharedFoldersResult_FailureImplCopyWithImpl<$Res>;
  @useResult
  $Res call({PeerRequestError field0});
}

/// @nodoc
class __$$BrowseSharedFoldersResult_FailureImplCopyWithImpl<$Res>
    extends _$BrowseSharedFoldersResultCopyWithImpl<$Res,
        _$BrowseSharedFoldersResult_FailureImpl>
    implements _$$BrowseSharedFoldersResult_FailureImplCopyWith<$Res> {
  __$$BrowseSharedFoldersResult_FailureImplCopyWithImpl(
      _$BrowseSharedFoldersResult_FailureImpl _value,
      $Res Function(_$BrowseSharedFoldersResult_FailureImpl) _then)
      : super(_value, _then);

  /// Create a copy of BrowseSharedFoldersResult
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$BrowseSharedFoldersResult_FailureImpl(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as PeerRequestError,
    ));
  }
}

/// @nodoc

class _$BrowseSharedFoldersResult_FailureImpl
    extends BrowseSharedFoldersResult_Failure {
  const _$BrowseSharedFoldersResult_FailureImpl(this.field0) : super._();

  @override
  final PeerRequestError field0;

  @override
  String toString() {
    return 'BrowseSharedFoldersResult.failure(field0: $field0)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$BrowseSharedFoldersResult_FailureImpl &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  /// Create a copy of BrowseSharedFoldersResult
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$BrowseSharedFoldersResult_FailureImplCopyWith<
          _$BrowseSharedFoldersResult_FailureImpl>
      get copyWith => __$$BrowseSharedFoldersResult_FailureImplCopyWithImpl<
          _$BrowseSharedFoldersResult_FailureImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(List<RemoteSharedFolder> field0) success,
    required TResult Function(PeerRequestError field0) failure,
  }) {
    return failure(field0);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(List<RemoteSharedFolder> field0)? success,
    TResult? Function(PeerRequestError field0)? failure,
  }) {
    return failure?.call(field0);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(List<RemoteSharedFolder> field0)? success,
    TResult Function(PeerRequestError field0)? failure,
    required TResult orElse(),
  }) {
    if (failure != null) {
      return failure(field0);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(BrowseSharedFoldersResult_Success value) success,
    required TResult Function(BrowseSharedFoldersResult_Failure value) failure,
  }) {
    return failure(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(BrowseSharedFoldersResult_Success value)? success,
    TResult? Function(BrowseSharedFoldersResult_Failure value)? failure,
  }) {
    return failure?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(BrowseSharedFoldersResult_Success value)? success,
    TResult Function(BrowseSharedFoldersResult_Failure value)? failure,
    required TResult orElse(),
  }) {
    if (failure != null) {
      return failure(this);
    }
    return orElse();
  }
}

abstract class BrowseSharedFoldersResult_Failure
    extends BrowseSharedFoldersResult {
  const factory BrowseSharedFoldersResult_Failure(
      final PeerRequestError field0) = _$BrowseSharedFoldersResult_FailureImpl;
  const BrowseSharedFoldersResult_Failure._() : super._();

  @override
  PeerRequestError get field0;

  /// Create a copy of BrowseSharedFoldersResult
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$BrowseSharedFoldersResult_FailureImplCopyWith<
          _$BrowseSharedFoldersResult_FailureImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
mixin _$FolderPairRequestOutcome {
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(
            String folderGuid, String name, String? remotePath)
        approved,
    required TResult Function(String field0) rejected,
    required TResult Function() pending,
    required TResult Function(PeerRequestError field0) failure,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String folderGuid, String name, String? remotePath)?
        approved,
    TResult? Function(String field0)? rejected,
    TResult? Function()? pending,
    TResult? Function(PeerRequestError field0)? failure,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String folderGuid, String name, String? remotePath)?
        approved,
    TResult Function(String field0)? rejected,
    TResult Function()? pending,
    TResult Function(PeerRequestError field0)? failure,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(FolderPairRequestOutcome_Approved value) approved,
    required TResult Function(FolderPairRequestOutcome_Rejected value) rejected,
    required TResult Function(FolderPairRequestOutcome_Pending value) pending,
    required TResult Function(FolderPairRequestOutcome_Failure value) failure,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(FolderPairRequestOutcome_Approved value)? approved,
    TResult? Function(FolderPairRequestOutcome_Rejected value)? rejected,
    TResult? Function(FolderPairRequestOutcome_Pending value)? pending,
    TResult? Function(FolderPairRequestOutcome_Failure value)? failure,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(FolderPairRequestOutcome_Approved value)? approved,
    TResult Function(FolderPairRequestOutcome_Rejected value)? rejected,
    TResult Function(FolderPairRequestOutcome_Pending value)? pending,
    TResult Function(FolderPairRequestOutcome_Failure value)? failure,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class $FolderPairRequestOutcomeCopyWith<$Res> {
  factory $FolderPairRequestOutcomeCopyWith(FolderPairRequestOutcome value,
          $Res Function(FolderPairRequestOutcome) then) =
      _$FolderPairRequestOutcomeCopyWithImpl<$Res, FolderPairRequestOutcome>;
}

/// @nodoc
class _$FolderPairRequestOutcomeCopyWithImpl<$Res,
        $Val extends FolderPairRequestOutcome>
    implements $FolderPairRequestOutcomeCopyWith<$Res> {
  _$FolderPairRequestOutcomeCopyWithImpl(this._value, this._then);

  // ignore: unused_field
  final $Val _value;
  // ignore: unused_field
  final $Res Function($Val) _then;

  /// Create a copy of FolderPairRequestOutcome
  /// with the given fields replaced by the non-null parameter values.
}

/// @nodoc
abstract class _$$FolderPairRequestOutcome_ApprovedImplCopyWith<$Res> {
  factory _$$FolderPairRequestOutcome_ApprovedImplCopyWith(
          _$FolderPairRequestOutcome_ApprovedImpl value,
          $Res Function(_$FolderPairRequestOutcome_ApprovedImpl) then) =
      __$$FolderPairRequestOutcome_ApprovedImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String folderGuid, String name, String? remotePath});
}

/// @nodoc
class __$$FolderPairRequestOutcome_ApprovedImplCopyWithImpl<$Res>
    extends _$FolderPairRequestOutcomeCopyWithImpl<$Res,
        _$FolderPairRequestOutcome_ApprovedImpl>
    implements _$$FolderPairRequestOutcome_ApprovedImplCopyWith<$Res> {
  __$$FolderPairRequestOutcome_ApprovedImplCopyWithImpl(
      _$FolderPairRequestOutcome_ApprovedImpl _value,
      $Res Function(_$FolderPairRequestOutcome_ApprovedImpl) _then)
      : super(_value, _then);

  /// Create a copy of FolderPairRequestOutcome
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? folderGuid = null,
    Object? name = null,
    Object? remotePath = freezed,
  }) {
    return _then(_$FolderPairRequestOutcome_ApprovedImpl(
      folderGuid: null == folderGuid
          ? _value.folderGuid
          : folderGuid // ignore: cast_nullable_to_non_nullable
              as String,
      name: null == name
          ? _value.name
          : name // ignore: cast_nullable_to_non_nullable
              as String,
      remotePath: freezed == remotePath
          ? _value.remotePath
          : remotePath // ignore: cast_nullable_to_non_nullable
              as String?,
    ));
  }
}

/// @nodoc

class _$FolderPairRequestOutcome_ApprovedImpl
    extends FolderPairRequestOutcome_Approved {
  const _$FolderPairRequestOutcome_ApprovedImpl(
      {required this.folderGuid, required this.name, this.remotePath})
      : super._();

  @override
  final String folderGuid;
  @override
  final String name;
  @override
  final String? remotePath;

  @override
  String toString() {
    return 'FolderPairRequestOutcome.approved(folderGuid: $folderGuid, name: $name, remotePath: $remotePath)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$FolderPairRequestOutcome_ApprovedImpl &&
            (identical(other.folderGuid, folderGuid) ||
                other.folderGuid == folderGuid) &&
            (identical(other.name, name) || other.name == name) &&
            (identical(other.remotePath, remotePath) ||
                other.remotePath == remotePath));
  }

  @override
  int get hashCode => Object.hash(runtimeType, folderGuid, name, remotePath);

  /// Create a copy of FolderPairRequestOutcome
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$FolderPairRequestOutcome_ApprovedImplCopyWith<
          _$FolderPairRequestOutcome_ApprovedImpl>
      get copyWith => __$$FolderPairRequestOutcome_ApprovedImplCopyWithImpl<
          _$FolderPairRequestOutcome_ApprovedImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(
            String folderGuid, String name, String? remotePath)
        approved,
    required TResult Function(String field0) rejected,
    required TResult Function() pending,
    required TResult Function(PeerRequestError field0) failure,
  }) {
    return approved(folderGuid, name, remotePath);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String folderGuid, String name, String? remotePath)?
        approved,
    TResult? Function(String field0)? rejected,
    TResult? Function()? pending,
    TResult? Function(PeerRequestError field0)? failure,
  }) {
    return approved?.call(folderGuid, name, remotePath);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String folderGuid, String name, String? remotePath)?
        approved,
    TResult Function(String field0)? rejected,
    TResult Function()? pending,
    TResult Function(PeerRequestError field0)? failure,
    required TResult orElse(),
  }) {
    if (approved != null) {
      return approved(folderGuid, name, remotePath);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(FolderPairRequestOutcome_Approved value) approved,
    required TResult Function(FolderPairRequestOutcome_Rejected value) rejected,
    required TResult Function(FolderPairRequestOutcome_Pending value) pending,
    required TResult Function(FolderPairRequestOutcome_Failure value) failure,
  }) {
    return approved(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(FolderPairRequestOutcome_Approved value)? approved,
    TResult? Function(FolderPairRequestOutcome_Rejected value)? rejected,
    TResult? Function(FolderPairRequestOutcome_Pending value)? pending,
    TResult? Function(FolderPairRequestOutcome_Failure value)? failure,
  }) {
    return approved?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(FolderPairRequestOutcome_Approved value)? approved,
    TResult Function(FolderPairRequestOutcome_Rejected value)? rejected,
    TResult Function(FolderPairRequestOutcome_Pending value)? pending,
    TResult Function(FolderPairRequestOutcome_Failure value)? failure,
    required TResult orElse(),
  }) {
    if (approved != null) {
      return approved(this);
    }
    return orElse();
  }
}

abstract class FolderPairRequestOutcome_Approved
    extends FolderPairRequestOutcome {
  const factory FolderPairRequestOutcome_Approved(
      {required final String folderGuid,
      required final String name,
      final String? remotePath}) = _$FolderPairRequestOutcome_ApprovedImpl;
  const FolderPairRequestOutcome_Approved._() : super._();

  String get folderGuid;
  String get name;
  String? get remotePath;

  /// Create a copy of FolderPairRequestOutcome
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$FolderPairRequestOutcome_ApprovedImplCopyWith<
          _$FolderPairRequestOutcome_ApprovedImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$FolderPairRequestOutcome_RejectedImplCopyWith<$Res> {
  factory _$$FolderPairRequestOutcome_RejectedImplCopyWith(
          _$FolderPairRequestOutcome_RejectedImpl value,
          $Res Function(_$FolderPairRequestOutcome_RejectedImpl) then) =
      __$$FolderPairRequestOutcome_RejectedImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String field0});
}

/// @nodoc
class __$$FolderPairRequestOutcome_RejectedImplCopyWithImpl<$Res>
    extends _$FolderPairRequestOutcomeCopyWithImpl<$Res,
        _$FolderPairRequestOutcome_RejectedImpl>
    implements _$$FolderPairRequestOutcome_RejectedImplCopyWith<$Res> {
  __$$FolderPairRequestOutcome_RejectedImplCopyWithImpl(
      _$FolderPairRequestOutcome_RejectedImpl _value,
      $Res Function(_$FolderPairRequestOutcome_RejectedImpl) _then)
      : super(_value, _then);

  /// Create a copy of FolderPairRequestOutcome
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$FolderPairRequestOutcome_RejectedImpl(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$FolderPairRequestOutcome_RejectedImpl
    extends FolderPairRequestOutcome_Rejected {
  const _$FolderPairRequestOutcome_RejectedImpl(this.field0) : super._();

  @override
  final String field0;

  @override
  String toString() {
    return 'FolderPairRequestOutcome.rejected(field0: $field0)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$FolderPairRequestOutcome_RejectedImpl &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  /// Create a copy of FolderPairRequestOutcome
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$FolderPairRequestOutcome_RejectedImplCopyWith<
          _$FolderPairRequestOutcome_RejectedImpl>
      get copyWith => __$$FolderPairRequestOutcome_RejectedImplCopyWithImpl<
          _$FolderPairRequestOutcome_RejectedImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(
            String folderGuid, String name, String? remotePath)
        approved,
    required TResult Function(String field0) rejected,
    required TResult Function() pending,
    required TResult Function(PeerRequestError field0) failure,
  }) {
    return rejected(field0);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String folderGuid, String name, String? remotePath)?
        approved,
    TResult? Function(String field0)? rejected,
    TResult? Function()? pending,
    TResult? Function(PeerRequestError field0)? failure,
  }) {
    return rejected?.call(field0);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String folderGuid, String name, String? remotePath)?
        approved,
    TResult Function(String field0)? rejected,
    TResult Function()? pending,
    TResult Function(PeerRequestError field0)? failure,
    required TResult orElse(),
  }) {
    if (rejected != null) {
      return rejected(field0);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(FolderPairRequestOutcome_Approved value) approved,
    required TResult Function(FolderPairRequestOutcome_Rejected value) rejected,
    required TResult Function(FolderPairRequestOutcome_Pending value) pending,
    required TResult Function(FolderPairRequestOutcome_Failure value) failure,
  }) {
    return rejected(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(FolderPairRequestOutcome_Approved value)? approved,
    TResult? Function(FolderPairRequestOutcome_Rejected value)? rejected,
    TResult? Function(FolderPairRequestOutcome_Pending value)? pending,
    TResult? Function(FolderPairRequestOutcome_Failure value)? failure,
  }) {
    return rejected?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(FolderPairRequestOutcome_Approved value)? approved,
    TResult Function(FolderPairRequestOutcome_Rejected value)? rejected,
    TResult Function(FolderPairRequestOutcome_Pending value)? pending,
    TResult Function(FolderPairRequestOutcome_Failure value)? failure,
    required TResult orElse(),
  }) {
    if (rejected != null) {
      return rejected(this);
    }
    return orElse();
  }
}

abstract class FolderPairRequestOutcome_Rejected
    extends FolderPairRequestOutcome {
  const factory FolderPairRequestOutcome_Rejected(final String field0) =
      _$FolderPairRequestOutcome_RejectedImpl;
  const FolderPairRequestOutcome_Rejected._() : super._();

  String get field0;

  /// Create a copy of FolderPairRequestOutcome
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$FolderPairRequestOutcome_RejectedImplCopyWith<
          _$FolderPairRequestOutcome_RejectedImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$FolderPairRequestOutcome_PendingImplCopyWith<$Res> {
  factory _$$FolderPairRequestOutcome_PendingImplCopyWith(
          _$FolderPairRequestOutcome_PendingImpl value,
          $Res Function(_$FolderPairRequestOutcome_PendingImpl) then) =
      __$$FolderPairRequestOutcome_PendingImplCopyWithImpl<$Res>;
}

/// @nodoc
class __$$FolderPairRequestOutcome_PendingImplCopyWithImpl<$Res>
    extends _$FolderPairRequestOutcomeCopyWithImpl<$Res,
        _$FolderPairRequestOutcome_PendingImpl>
    implements _$$FolderPairRequestOutcome_PendingImplCopyWith<$Res> {
  __$$FolderPairRequestOutcome_PendingImplCopyWithImpl(
      _$FolderPairRequestOutcome_PendingImpl _value,
      $Res Function(_$FolderPairRequestOutcome_PendingImpl) _then)
      : super(_value, _then);

  /// Create a copy of FolderPairRequestOutcome
  /// with the given fields replaced by the non-null parameter values.
}

/// @nodoc

class _$FolderPairRequestOutcome_PendingImpl
    extends FolderPairRequestOutcome_Pending {
  const _$FolderPairRequestOutcome_PendingImpl() : super._();

  @override
  String toString() {
    return 'FolderPairRequestOutcome.pending()';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$FolderPairRequestOutcome_PendingImpl);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(
            String folderGuid, String name, String? remotePath)
        approved,
    required TResult Function(String field0) rejected,
    required TResult Function() pending,
    required TResult Function(PeerRequestError field0) failure,
  }) {
    return pending();
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String folderGuid, String name, String? remotePath)?
        approved,
    TResult? Function(String field0)? rejected,
    TResult? Function()? pending,
    TResult? Function(PeerRequestError field0)? failure,
  }) {
    return pending?.call();
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String folderGuid, String name, String? remotePath)?
        approved,
    TResult Function(String field0)? rejected,
    TResult Function()? pending,
    TResult Function(PeerRequestError field0)? failure,
    required TResult orElse(),
  }) {
    if (pending != null) {
      return pending();
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(FolderPairRequestOutcome_Approved value) approved,
    required TResult Function(FolderPairRequestOutcome_Rejected value) rejected,
    required TResult Function(FolderPairRequestOutcome_Pending value) pending,
    required TResult Function(FolderPairRequestOutcome_Failure value) failure,
  }) {
    return pending(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(FolderPairRequestOutcome_Approved value)? approved,
    TResult? Function(FolderPairRequestOutcome_Rejected value)? rejected,
    TResult? Function(FolderPairRequestOutcome_Pending value)? pending,
    TResult? Function(FolderPairRequestOutcome_Failure value)? failure,
  }) {
    return pending?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(FolderPairRequestOutcome_Approved value)? approved,
    TResult Function(FolderPairRequestOutcome_Rejected value)? rejected,
    TResult Function(FolderPairRequestOutcome_Pending value)? pending,
    TResult Function(FolderPairRequestOutcome_Failure value)? failure,
    required TResult orElse(),
  }) {
    if (pending != null) {
      return pending(this);
    }
    return orElse();
  }
}

abstract class FolderPairRequestOutcome_Pending
    extends FolderPairRequestOutcome {
  const factory FolderPairRequestOutcome_Pending() =
      _$FolderPairRequestOutcome_PendingImpl;
  const FolderPairRequestOutcome_Pending._() : super._();
}

/// @nodoc
abstract class _$$FolderPairRequestOutcome_FailureImplCopyWith<$Res> {
  factory _$$FolderPairRequestOutcome_FailureImplCopyWith(
          _$FolderPairRequestOutcome_FailureImpl value,
          $Res Function(_$FolderPairRequestOutcome_FailureImpl) then) =
      __$$FolderPairRequestOutcome_FailureImplCopyWithImpl<$Res>;
  @useResult
  $Res call({PeerRequestError field0});
}

/// @nodoc
class __$$FolderPairRequestOutcome_FailureImplCopyWithImpl<$Res>
    extends _$FolderPairRequestOutcomeCopyWithImpl<$Res,
        _$FolderPairRequestOutcome_FailureImpl>
    implements _$$FolderPairRequestOutcome_FailureImplCopyWith<$Res> {
  __$$FolderPairRequestOutcome_FailureImplCopyWithImpl(
      _$FolderPairRequestOutcome_FailureImpl _value,
      $Res Function(_$FolderPairRequestOutcome_FailureImpl) _then)
      : super(_value, _then);

  /// Create a copy of FolderPairRequestOutcome
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$FolderPairRequestOutcome_FailureImpl(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as PeerRequestError,
    ));
  }
}

/// @nodoc

class _$FolderPairRequestOutcome_FailureImpl
    extends FolderPairRequestOutcome_Failure {
  const _$FolderPairRequestOutcome_FailureImpl(this.field0) : super._();

  @override
  final PeerRequestError field0;

  @override
  String toString() {
    return 'FolderPairRequestOutcome.failure(field0: $field0)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$FolderPairRequestOutcome_FailureImpl &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  /// Create a copy of FolderPairRequestOutcome
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$FolderPairRequestOutcome_FailureImplCopyWith<
          _$FolderPairRequestOutcome_FailureImpl>
      get copyWith => __$$FolderPairRequestOutcome_FailureImplCopyWithImpl<
          _$FolderPairRequestOutcome_FailureImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(
            String folderGuid, String name, String? remotePath)
        approved,
    required TResult Function(String field0) rejected,
    required TResult Function() pending,
    required TResult Function(PeerRequestError field0) failure,
  }) {
    return failure(field0);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String folderGuid, String name, String? remotePath)?
        approved,
    TResult? Function(String field0)? rejected,
    TResult? Function()? pending,
    TResult? Function(PeerRequestError field0)? failure,
  }) {
    return failure?.call(field0);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String folderGuid, String name, String? remotePath)?
        approved,
    TResult Function(String field0)? rejected,
    TResult Function()? pending,
    TResult Function(PeerRequestError field0)? failure,
    required TResult orElse(),
  }) {
    if (failure != null) {
      return failure(field0);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(FolderPairRequestOutcome_Approved value) approved,
    required TResult Function(FolderPairRequestOutcome_Rejected value) rejected,
    required TResult Function(FolderPairRequestOutcome_Pending value) pending,
    required TResult Function(FolderPairRequestOutcome_Failure value) failure,
  }) {
    return failure(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(FolderPairRequestOutcome_Approved value)? approved,
    TResult? Function(FolderPairRequestOutcome_Rejected value)? rejected,
    TResult? Function(FolderPairRequestOutcome_Pending value)? pending,
    TResult? Function(FolderPairRequestOutcome_Failure value)? failure,
  }) {
    return failure?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(FolderPairRequestOutcome_Approved value)? approved,
    TResult Function(FolderPairRequestOutcome_Rejected value)? rejected,
    TResult Function(FolderPairRequestOutcome_Pending value)? pending,
    TResult Function(FolderPairRequestOutcome_Failure value)? failure,
    required TResult orElse(),
  }) {
    if (failure != null) {
      return failure(this);
    }
    return orElse();
  }
}

abstract class FolderPairRequestOutcome_Failure
    extends FolderPairRequestOutcome {
  const factory FolderPairRequestOutcome_Failure(
      final PeerRequestError field0) = _$FolderPairRequestOutcome_FailureImpl;
  const FolderPairRequestOutcome_Failure._() : super._();

  PeerRequestError get field0;

  /// Create a copy of FolderPairRequestOutcome
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$FolderPairRequestOutcome_FailureImplCopyWith<
          _$FolderPairRequestOutcome_FailureImpl>
      get copyWith => throw _privateConstructorUsedError;
}
