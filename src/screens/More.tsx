// Everything that is not the daily job, in one quiet list.
//
// No subtitles: a shopkeeper recognises «چک‌ها» and «گزارش» without being told
// what they hold, and every line of explanation is a line he has to read.

import { useNav } from "../nav";
import { APP_VERSION } from "../version";
import { Body, Card, Head, Icon, Row, Screen } from "../ui/kit";



export default function More() {
  const nav = useNav();

  return (
    <Screen>
      <Head title="بیشتر" />
      <Body>
        <Card tight>
          <Row
            leading={<Icon name="search" />}
            title="جست‌وجو"
            onClick={() => nav.go({ name: "search" })}
          />
          <Row
            leading={<Icon name="book" />}
            title="کالاها"
            onClick={() => nav.go({ name: "products" })}
          />
          <Row
            leading={<Icon name="check" />}
            title="چک‌ها"
            onClick={() => nav.go({ name: "cheques" })}
          />
          <Row
            leading={<Icon name="edit" />}
            title="یادداشت‌ها"
            onClick={() => nav.go({ name: "notes" })}
          />
          <Row
            leading={<Icon name="calc" />}
            title="برگه جمع"
            onClick={() => nav.go({ name: "sums" })}
          />
          <Row
            leading={<Icon name="today" />}
            title="گزارش"
            onClick={() => nav.go({ name: "reports" })}
          />
        </Card>

        <Card tight>
          <Row
            leading={<Icon name="send" />}
            title="تنظیمات"
            onClick={() => nav.go({ name: "settings" })}
          />
        </Card>

        <div className="more-foot faint tiny">
          دفتر {APP_VERSION.replace(/[0-9]/g, (d) => "۰۱۲۳۴۵۶۷۸۹"[Number(d)]!)}
        </div>
      </Body>
    </Screen>
  );
}
