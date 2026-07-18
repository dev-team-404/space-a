"""도메인 에러. code는 docs/design/room-visit.md §4의 에러 코드와 맞춘다."""


class RoomServerError(Exception):
    code = "invalid_request"


class InvalidRequest(RoomServerError):
    code = "invalid_request"


class Unauthorized(RoomServerError):
    code = "unauthorized"


class Forbidden(RoomServerError):
    code = "forbidden"


class NotFound(RoomServerError):
    code = "not_found"


class CellTaken(RoomServerError):
    code = "cell_taken"
