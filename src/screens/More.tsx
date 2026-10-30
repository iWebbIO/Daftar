// Everything that is not the daily job, gathered in one quiet list.

import { useNav } from "../nav";
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
            sub="نام، کالا، مبلغ، یادداشت"
            onClick={() => nav.go({ name: "search" })}
          />
          <Row
            leading={<Icon name="book" />}
            title="کالاها"
            sub="قیمت‌ها و واحدها"
            onClick={() => nav.go({ name: "products" })}
          />
          <Row
            leading={<Icon name="check" />}
            title="چک‌ها"
            sub="سررسید و وضعیت"
            onClick={() => nav.go({ name: "cheques" })}
          />
          <Row
            leading={<Icon name="edit" />}
            title="یادداشت‌ها"
            sub="نوشته‌های روزانه"
            onClick={() => nav.go({ name: "notes" })}
          />
          <Row
            leading={<Icon name="calc" />}
            title="برگه جمع"
            sub="ستون‌های عددی"
            onClick={() => nav.go({ name: "sums" })}
          />
          <Row
            leading={<Icon name="calc" />}
            title="ماشین‌حساب"
            sub="جمع و تفریق سریع"
            onClick={() => nav.go({ name: "calc" })}
          />
          <Row
            leading={<Icon name="today" />}
            title="گزارش"
            sub="فروش، طلب، کهنه شدن بدهی"
            onClick={() => nav.go({ name: "reports" })}
          />
        </Card>

        <Card tight>
          <Row
            leading={<Icon name="send" />}
            title="تنظیمات و پشتیبان"
            sub="ربات تلگرام، نام فروشگاه، نسخه پشتیبان"
            onClick={() => nav.go({ name: "settings" })}
          />
        </Card>

        <div className="more-foot faint tiny">دفتر · نسخه ۰.۱</div>
      </Body>
    </Screen>
  );
}
