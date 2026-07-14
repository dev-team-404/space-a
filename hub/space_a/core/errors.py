"""도메인 에러. code는 C1/C4 계약의 에러 코드와 맞춘다."""


class SpaceAError(Exception):
    code = "invalid_request"


class InvalidRequest(SpaceAError):
    code = "invalid_request"


class Unauthorized(SpaceAError):
    code = "unauthorized"


class NotFound(SpaceAError):
    code = "not_found"


class Forbidden(SpaceAError):
    code = "forbidden"
