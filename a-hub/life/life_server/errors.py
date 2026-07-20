"""도메인 에러. code는 docs/design/life-visit.md §4의 에러 코드와 맞춘다."""


class LifeServerError(Exception):
    code = "invalid_request"


class InvalidRequest(LifeServerError):
    code = "invalid_request"


class Unauthorized(LifeServerError):
    code = "unauthorized"


class Forbidden(LifeServerError):
    code = "forbidden"


class NotFound(LifeServerError):
    code = "not_found"


class CellTaken(LifeServerError):
    code = "cell_taken"
