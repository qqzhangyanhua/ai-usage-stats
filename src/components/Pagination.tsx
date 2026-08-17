import { Button } from "./ui/Button";

export function Pagination({
  page,
  pageCount,
  totalCount,
  onPageChange,
}: {
  page: number;
  pageCount: number;
  totalCount: number;
  onPageChange: (page: number) => void;
}) {
  if (pageCount <= 1) {
    return null;
  }
  return (
    <nav className="pagination" aria-label="分页">
      <span className="pagination-total">共 {totalCount} 条</span>
      <div className="pagination-controls">
        <Button disabled={page <= 1} onClick={() => onPageChange(page - 1)} aria-label="上一页">
          上一页
        </Button>
        <span className="pagination-info" aria-current="page">
          第 {page} / {pageCount} 页
        </span>
        <Button
          disabled={page >= pageCount}
          onClick={() => onPageChange(page + 1)}
          aria-label="下一页"
        >
          下一页
        </Button>
      </div>
    </nav>
  );
}
