#include "percentbardelegate.h"

#include <QApplication>
#include <QDebug>


PercentBarDelegate::PercentBarDelegate(int max, QObject* parent)
	: QStyledItemDelegate(parent)
{
	m_maxValue = max;
}


void PercentBarDelegate::paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const
{
	bool isInt;
	int num = index.data(Qt::DisplayRole).toInt(&isInt);

	if (isInt)
	{
		// paint correct background
		QStyleOptionViewItem bg(option);
		bg.text.clear();
		QApplication::style()->drawControl(QStyle::CE_ItemViewItem, &bg, painter, option.widget);

		QStyleOptionProgressBar bar;
		bar.rect = option.rect;
		bar.palette = option.palette;
		bar.minimum = 0;
		bar.maximum = m_maxValue;
		bar.text = QString::number(num);
		bar.textVisible = true;
		bar.progress = num;
		QApplication::style()->drawControl(QStyle::CE_ProgressBar, &bar, painter, option.widget);
	}
	else
		QStyledItemDelegate::paint(painter, option, index);
}
