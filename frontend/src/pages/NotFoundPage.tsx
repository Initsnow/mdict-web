import { Link } from "react-router-dom";
import { Button } from "@/components/ui/button";
import { ArrowLeft } from "lucide-react";

export function NotFoundPage() {
  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-4 px-4 text-center">
      <p className="text-5xl font-semibold tracking-tight text-muted-foreground/30">
        404
      </p>
      <div>
        <h1 className="text-lg font-semibold">Page not found</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          The page you're looking for doesn't exist.
        </p>
      </div>
      <Button variant="outline" size="sm" render={<Link to="/" />}>
        <ArrowLeft className="mr-1.5 h-3.5 w-3.5" />
        Back to home
      </Button>
    </div>
  );
}
