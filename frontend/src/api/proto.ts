import {
  create,
  type DescMessage,
  fromJson,
  type JsonValue,
  type MessageInitShape,
  type MessageShape,
  toJson,
} from "@bufbuild/protobuf"

/** Proto JSON as both backends send it: snake_case keys. */
export function encode<Desc extends DescMessage>(
  schema: Desc,
  init: MessageInitShape<Desc>,
): JsonValue {
  return toJson(schema, create(schema, init), { useProtoFieldName: true })
}

export function decode<Desc extends DescMessage>(schema: Desc, json: JsonValue): MessageShape<Desc> {
  return fromJson(schema, json, { ignoreUnknownFields: true })
}
